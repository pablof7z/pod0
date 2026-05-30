import Foundation

// MARK: - HTTP capability
//
// iOS half of the kernel-side HTTP capability (namespace
// `nmp.http.capability`). The Rust contract lives in
// `apps/podcast-feeds/src/http.rs` (M5 introduced it; the canonical
// `nmp-core::capability::http` hasn't landed upstream yet, so the
// podcast-player crate graph owns the schema for now). The
// `nmp-app-podcast::capability::http` module re-exports it for symmetry
// with `capability::audio` / `capability::download`.
//
// The kernel never makes HTTP calls directly; it routes them through the
// capability socket so the platform supplies the transport. This is the
// `URLSession` implementation, the second capability after `KeychainCapability`.
//
// Doctrine (docs/product-spec/doctrine.md):
//   D6 — errors never cross the boundary as exceptions. A DNS/TLS/timeout
//        failure, a malformed request, or the capability being stopped all
//        return a populated `HttpResult.error` inside the envelope; this type
//        never `throw`s across `handle(_:)` / `handleJSON(_:)`.
//   D7 — a capability reports and executes; it never decides policy. It
//        performs the exact GET/POST the kernel asks for and reports the
//        result. *Which* URL to call and *what* to do with the bolt11 invoice
//        an LNURL-pay round-trip yields are NIP-57 (kernel) decisions.
//
// SYNCHRONOUS SOCKET — the capability socket
// (`crates/nmp-core/src/capability_socket.rs`) is synchronous: Rust calls the
// native callback and blocks the actor thread until it returns. So this
// implementation must block too — it drives `URLSession` and waits on a
// `DispatchSemaphore`. For a rare user-triggered action like a NIP-57 zap
// (~500ms per HTTP call) this is an acceptable MVP trade-off; see
// `docs/decisions/0023-http-capability-synchronous-socket.md`. The blocking
// wait is safe because Rust invokes this callback from the actor thread, never
// the main thread, and `URLSession` completion handlers run on the session's
// own delegate queue (a background `OperationQueue`), not the calling thread —
// so `semaphore.wait()` here can never deadlock against the completion.

// MARK: - HTTP payload / result vocabulary

/// HTTP verb. Mirrors the Rust `HttpMethod` (`#[serde(rename_all =
/// "UPPERCASE")]` — wire values `"GET"` / `"POST"`).
enum HttpMethod: String, Decodable {
    case get = "GET"
    case post = "POST"
}

/// Capability-private request payload — the decoded `payload_json`.
/// Mirrors the Rust `HttpRequest`.
struct HttpRequest: Decodable {
    let method: HttpMethod
    let url: String
    /// Header `[name, value]` pairs. Absent from the wire ⇒ empty.
    let headers: [[String]]
    /// Request body as a UTF-8 string — present for a text POST (e.g. an LNURL
    /// callback or a JSON payload), absent for GET. Mutually exclusive with
    /// `bodyBase64`: a UTF-8 string body cannot carry arbitrary binary bytes.
    let body: String?
    /// Request body as standard-alphabet base64 (`+/`, padded). Present when the
    /// kernel needs to send *binary* bytes that don't survive a UTF-8 round-trip
    /// — e.g. the Blossom blob upload (`apps/nmp-app-podcast/src/blossom.rs`),
    /// which SHA-256s the raw audio file and base64-encodes the bytes so they
    /// can transit this capability intact. When present it takes precedence over
    /// `body` and is decoded back to raw `Data` before being sent as the HTTP
    /// request body. The wire field is purely additive: callers that only send
    /// UTF-8 bodies omit it and behave exactly as before.
    let bodyBase64: String?

    enum CodingKeys: String, CodingKey {
        case method
        case url
        case headers
        case body
        case bodyBase64 = "body_base64"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        method = try c.decode(HttpMethod.self, forKey: .method)
        url = try c.decode(String.self, forKey: .url)
        headers = try c.decodeIfPresent([[String]].self, forKey: .headers) ?? []
        body = try c.decodeIfPresent(String.self, forKey: .body)
        bodyBase64 = try c.decodeIfPresent(String.self, forKey: .bodyBase64)
    }
}

/// Capability-private result payload — the encoded `result_json`. Mirrors the
/// Rust `HttpResult` (`#[serde(tag = "status", rename_all = "snake_case")]`):
///   `{"status":"ok","status_code":200,"headers":[["ETag","…"]],"body":"…"}`
///   `{"status":"error","message":"…"}`
///
/// There is no error *exception*: a transport failure is data (`status ==
/// "error"`), satisfying D6.
///
/// **M5 — response headers on `Ok`.** The `headers` field on `.ok` was added
/// to round-trip `ETag` / `Last-Modified` to Rust callers (the FeedClient
/// conditional-GET path can't read response headers otherwise). The wire
/// addition is purely additive — older Rust decoders that don't know the
/// field skip it, and an empty header set is omitted to keep the payload
/// tidy. This is a podcast-player-side extension of Chirp's `HttpCapability`
/// contract; if Chirp adopts the same shape later we can collapse the two.
enum HttpResult: Encodable {
    /// Transport succeeded — `statusCode` is the raw HTTP status (a 200 and a
    /// 404 are both `ok`; interpreting it is the caller's policy, D7).
    /// `headers` is the response's `allHeaderFields` flattened to ordered
    /// `[name, value]` pairs (preserves case from the server response).
    case ok(statusCode: UInt16, headers: [[String]], body: String)
    /// Transport-level failure (DNS, TLS, timeout, malformed request, …).
    case error(message: String)

    enum CodingKeys: String, CodingKey {
        case status
        case statusCode = "status_code"
        case headers
        case body
        case message
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .ok(statusCode, headers, body):
            try c.encode("ok", forKey: .status)
            try c.encode(statusCode, forKey: .statusCode)
            // Match the Rust `skip_serializing_if = "Vec::is_empty"`: omit the
            // headers field entirely when empty so the wire payload stays
            // identical to the pre-M5 shape for non-HTTP responses
            // (`file://` short-circuit, etc.).
            if !headers.isEmpty {
                try c.encode(headers, forKey: .headers)
            }
            try c.encode(body, forKey: .body)
        case let .error(message):
            try c.encode("error", forKey: .status)
            try c.encode(message, forKey: .message)
        }
    }
}

// MARK: - URLSession-backed capability

/// `URLSession` implementation of the HTTP capability.
///
/// Performs the GET/POST the kernel asks for and reports the raw result. The
/// call blocks the calling (actor) thread until the HTTP round-trip completes —
/// see the synchronous-socket note at the top of this file.
final class HttpCapability {
    static let namespace = "nmp.http.capability"

    /// Wall-clock ceiling for a single HTTP call. An LNURL endpoint that never
    /// answers must not stall the actor thread forever — on expiry the request
    /// is reported as a timeout `HttpResult.error` (D6).
    private let timeout: TimeInterval

    /// Dedicated session so completions run on a private background
    /// `OperationQueue` (never the main queue) — the `semaphore.wait()` below
    /// can therefore never deadlock against the completion handler.
    private let session: URLSession

    private var started = false

    init(timeout: TimeInterval = 20) {
        self.timeout = timeout
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = timeout
        config.waitsForConnectivity = false
        let queue = OperationQueue()
        queue.name = "io.f7z.podcast.HttpCapability"
        self.session = URLSession(configuration: config, delegate: nil, delegateQueue: queue)
    }

    // MARK: Lifecycle (idempotent)

    /// Idempotent. Marks the capability active so requests are served.
    func start() {
        started = true
    }

    /// Idempotent. Marks the handler inactive so late requests are rejected as
    /// data (`HttpResult.error`), not crashes. Does not cancel in-flight calls.
    func stop() {
        started = false
    }

    var isStarted: Bool { started }

    // MARK: Envelope handling (never throws — D6)

    /// Decode → execute → encode. Any failure (malformed request, capability
    /// stopped, transport error) is returned inside the envelope's
    /// `result_json`, never raised.
    func handle(_ request: CapabilityRequest) -> CapabilityEnvelope {
        let result = process(request)
        let resultJSON = Self.encode(result)
            ?? Self.encode(HttpResult.error(message: "result-encode-failed"))
            ?? "{\"status\":\"error\",\"message\":\"result-encode-failed\"}"
        return CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: request.correlationID,
            resultJSON: resultJSON)
    }

    /// Convenience entry point for FFI bridges that hand us the raw kernel
    /// `CapabilityRequest` JSON and want raw `CapabilityEnvelope` JSON back.
    /// Honors D6 end to end: malformed input yields an error envelope string.
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            let env = CapabilityEnvelope(
                namespace: Self.namespace,
                correlationID: "",
                resultJSON: Self.encode(HttpResult.error(message: "malformed-request"))
                    ?? "{\"status\":\"error\",\"message\":\"malformed-request\"}")
            return Self.encode(env) ?? "{}"
        }
        return Self.encode(handle(request)) ?? "{}"
    }

    // MARK: - Internals

    private func process(_ request: CapabilityRequest) -> HttpResult {
        guard started else { return .error(message: "capability-stopped") }
        guard
            let payload = request.payloadJSON.data(using: .utf8),
            let httpRequest = try? JSONDecoder().decode(HttpRequest.self, from: payload)
        else {
            return .error(message: "malformed-payload")
        }
        guard let url = URL(string: httpRequest.url) else {
            return .error(message: "invalid-url")
        }

        var urlRequest = URLRequest(url: url)
        urlRequest.httpMethod = httpRequest.method.rawValue
        urlRequest.timeoutInterval = timeout
        for pair in httpRequest.headers where pair.count == 2 {
            urlRequest.setValue(pair[1], forHTTPHeaderField: pair[0])
        }
        // Binary bodies arrive base64-encoded so they survive the UTF-8 wire
        // (the Rust `body` field is a `String`). `bodyBase64` wins over `body`
        // when both are somehow present — it is the only field that can carry
        // arbitrary bytes (e.g. a Blossom blob upload). A malformed base64
        // payload is reported as data, never silently sent as garbage (D6).
        if let bodyBase64 = httpRequest.bodyBase64 {
            guard let data = Data(base64Encoded: bodyBase64) else {
                return .error(message: "invalid-body-base64")
            }
            urlRequest.httpBody = data
        } else if let body = httpRequest.body {
            urlRequest.httpBody = body.data(using: .utf8)
        }

        return perform(urlRequest)
    }

    /// Lock-guarded one-shot result box. The completion handler (running on the
    /// session's background queue) writes once; `perform` reads once after the
    /// semaphore wait. The `NSLock` makes the cross-thread handoff explicit so
    /// the Swift 6 concurrency checker is satisfied — a bare captured `var`
    /// would be flagged as a data race even though the semaphore orders it.
    private final class ResultBox: @unchecked Sendable {
        private let lock = NSLock()
        private var value: HttpResult = .error(message: "no-response")

        func set(_ result: HttpResult) {
            lock.lock()
            value = result
            lock.unlock()
        }

        func get() -> HttpResult {
            lock.lock()
            defer { lock.unlock() }
            return value
        }
    }

    /// Drive `urlRequest` to completion synchronously. Blocks the calling
    /// thread on a `DispatchSemaphore` until the `URLSession` completion
    /// handler fires on the session's private background queue — safe because
    /// the two are different threads (see the file-level note).
    private func perform(_ urlRequest: URLRequest) -> HttpResult {
        let semaphore = DispatchSemaphore(value: 0)
        let box = ResultBox()

        let task = session.dataTask(with: urlRequest) { data, response, error in
            defer { semaphore.signal() }
            if let error {
                box.set(.error(message: "transport: \(error.localizedDescription)"))
                return
            }
            guard let http = response as? HTTPURLResponse else {
                box.set(.error(message: "non-http-response"))
                return
            }
            let body = data.flatMap { String(data: $0, encoding: .utf8) } ?? ""
            let headers = Self.headerPairs(from: http)
            box.set(.ok(
                statusCode: UInt16(clamping: http.statusCode),
                headers: headers,
                body: body))
        }
        task.resume()
        // A generous ceiling above `timeoutIntervalForRequest` so the session's
        // own timeout fires first; this `wait` deadline is the backstop.
        if semaphore.wait(timeout: .now() + timeout + 5) == .timedOut {
            task.cancel()
            return .error(message: "timeout")
        }
        return box.get()
    }

    /// Project `HTTPURLResponse.allHeaderFields` to ordered `[name, value]`
    /// pairs. Values aren't necessarily strings (the API types them as
    /// `[AnyHashable: Any]`); coerce non-string values via `String(describing:)`
    /// so an unusual header (`Content-Length` returned as `NSNumber` in some
    /// stacks) still round-trips as text.
    private static func headerPairs(from response: HTTPURLResponse) -> [[String]] {
        var pairs: [[String]] = []
        pairs.reserveCapacity(response.allHeaderFields.count)
        for (rawName, rawValue) in response.allHeaderFields {
            let name = rawName as? String ?? String(describing: rawName)
            let value = rawValue as? String ?? String(describing: rawValue)
            pairs.append([name, value])
        }
        return pairs
    }

#if DEBUG
    /// Open a Server-Sent Events stream for the given request.
    ///
    /// - Note: Not yet implemented. Filled in at M5.
    // TODO(M5): SSE streaming
    func openSseStream(_ requestJSON: String) -> String {
        fatalError("openSseStream not yet implemented — see TODO(M5)")
    }

    /// Open a WebSocket session for the given request.
    ///
    /// - Note: Not yet implemented. Filled in at M8.
    // TODO(M8): WebSocket streaming
    func openWebSocketSession(_ requestJSON: String) -> String {
        fatalError("openWebSocketSession not yet implemented — see TODO(M8)")
    }
#endif

    private static func encode<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
