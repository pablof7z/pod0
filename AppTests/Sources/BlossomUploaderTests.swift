import CryptoKit
import Foundation
import XCTest
@testable import Podcastr

// MARK: - BlossomUploaderTests
//
// Focused coverage for the in-memory Blossom upload transport
// (`App/Sources/Services/BlossomUploader.swift`) per issue #324's acceptance
// criterion: success/failure handling and the signing/upload boundary.
//
// No network and no kernel: HTTP is intercepted with a stub `URLProtocol` and
// signing is driven by an injected fake `NostrSigner`. These tests pin the
// transport contract that stays in Swift while the active-account upload path
// is blocked on the async sign-and-return capability
// (docs/BACKLOG.md → `blossom-active-account-upload-kernel`).
final class BlossomUploaderTests: XCTestCase {

    override func tearDown() {
        StubURLProtocol.handler = nil
        super.tearDown()
    }

    // MARK: Helpers

    /// A `URLSession` whose only protocol is the in-memory stub, so no request
    /// ever leaves the process.
    private func stubbedSession() -> URLSession {
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [StubURLProtocol.self]
        return URLSession(configuration: config)
    }

    private func uploader(session: URLSession) -> BlossomUploader {
        BlossomUploader(server: URL(string: "https://blossom.example")!, session: session)
    }

    // MARK: Success

    /// A 2xx response with a JSON descriptor parses the `url` field and returns
    /// it. Also asserts the request shape: PUT to `/upload`, content-type and
    /// content-length set, and the raw bytes as the body.
    func testSuccessfulUploadParsesDescriptorURL() async throws {
        let payload = Data("the-blob-bytes".utf8)
        let expectedHash = Data(SHA256.hash(data: payload)).hexString

        StubURLProtocol.handler = { request in
            XCTAssertEqual(request.httpMethod, "PUT")
            XCTAssertEqual(request.url?.path, "/upload")
            XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "image/jpeg")
            XCTAssertEqual(
                request.value(forHTTPHeaderField: "Content-Length"), String(payload.count))
            XCTAssertEqual(StubURLProtocol.bodyData(request), payload)

            let descriptor = #"{"url":"https://cdn.example/\#(expectedHash).jpg","sha256":"\#(expectedHash)","size":\#(payload.count),"type":"image/jpeg"}"#
            return (200, Data(descriptor.utf8), nil)
        }

        let url = try await uploader(session: stubbedSession())
            .upload(data: payload, contentType: "image/jpeg", signer: FakeSigner())

        XCTAssertEqual(url.absoluteString, "https://cdn.example/\(expectedHash).jpg")
    }

    // MARK: Failure — server rejection

    /// A non-2xx response surfaces `serverRejected`, preferring the BUD-01
    /// `X-Reason` header over the body.
    func testServerRejectionUsesXReasonHeader() async {
        StubURLProtocol.handler = { _ in
            (413, Data("ignored body".utf8), ["X-Reason": "blob too large"])
        }

        do {
            _ = try await uploader(session: stubbedSession())
                .upload(data: Data("x".utf8), contentType: "image/png", signer: FakeSigner())
            XCTFail("expected a serverRejected error")
        } catch let BlossomUploadError.serverRejected(reason) {
            XCTAssertEqual(reason, "blob too large")
        } catch {
            XCTFail("expected BlossomUploadError.serverRejected, got \(error)")
        }
    }

    /// When `X-Reason` is absent, the rejection reason falls back to the
    /// response body.
    func testServerRejectionFallsBackToBody() async {
        StubURLProtocol.handler = { _ in
            (400, Data("invalid auth".utf8), nil)
        }

        do {
            _ = try await uploader(session: stubbedSession())
                .upload(data: Data("x".utf8), contentType: "image/png", signer: FakeSigner())
            XCTFail("expected a serverRejected error")
        } catch let BlossomUploadError.serverRejected(reason) {
            XCTAssertEqual(reason, "invalid auth")
        } catch {
            XCTFail("expected BlossomUploadError.serverRejected, got \(error)")
        }
    }

    // MARK: Failure — malformed descriptor

    /// A 2xx response whose JSON has no `url` field surfaces `malformedDescriptor`.
    func testMalformedDescriptorThrows() async {
        StubURLProtocol.handler = { _ in
            (200, Data(#"{"sha256":"abc","size":3}"#.utf8), nil)
        }

        do {
            _ = try await uploader(session: stubbedSession())
                .upload(data: Data("x".utf8), contentType: "image/png", signer: FakeSigner())
            XCTFail("expected a malformedDescriptor error")
        } catch BlossomUploadError.malformedDescriptor {
            // expected
        } catch {
            XCTFail("expected BlossomUploadError.malformedDescriptor, got \(error)")
        }
    }

    // MARK: Signing boundary

    /// The `Authorization` header is `Nostr <base64>` where the base64 decodes
    /// to the exact signed event the injected signer returned — proving the
    /// transport carries the kernel-signed kind:24242 auth event verbatim and
    /// does no signing of its own.
    func testAuthorizationHeaderCarriesSignedEventFromSigner() async throws {
        let payload = Data("artwork".utf8)
        let expectedHash = Data(SHA256.hash(data: payload)).hexString
        let signer = FakeSigner()

        var capturedAuth: String?
        StubURLProtocol.handler = { request in
            capturedAuth = request.value(forHTTPHeaderField: "Authorization")
            return (200, Data(#"{"url":"https://cdn.example/blob"}"#.utf8), nil)
        }

        _ = try await uploader(session: stubbedSession())
            .upload(data: payload, contentType: "image/png", signer: signer)

        // The signer received a kind:24242 draft tagged with the payload hash.
        let draft = try XCTUnwrap(signer.lastDraft)
        XCTAssertEqual(draft.kind, 24242)
        XCTAssertTrue(draft.tags.contains(["t", "upload"]))
        XCTAssertTrue(draft.tags.contains(["x", expectedHash]))

        // The header base64 decodes to a JSON object equal to the signed event.
        let header = try XCTUnwrap(capturedAuth)
        XCTAssertTrue(header.hasPrefix("Nostr "))
        let b64 = String(header.dropFirst("Nostr ".count))
        let json = try XCTUnwrap(Data(base64Encoded: b64))
        let object = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: json) as? [String: Any])

        let signed = try XCTUnwrap(signer.lastSigned)
        XCTAssertEqual(object["id"] as? String, signed.id)
        XCTAssertEqual(object["pubkey"] as? String, signed.pubkey)
        XCTAssertEqual(object["sig"] as? String, signed.sig)
        XCTAssertEqual(object["kind"] as? Int, signed.kind)
        XCTAssertEqual(object["content"] as? String, signed.content)
    }

    /// A signer failure propagates and no HTTP request is attempted.
    func testSignerFailurePropagatesWithoutHTTP() async {
        var httpAttempted = false
        StubURLProtocol.handler = { _ in
            httpAttempted = true
            return (200, Data(#"{"url":"https://cdn.example/blob"}"#.utf8), nil)
        }

        let signer = FakeSigner()
        signer.signError = NostrSignerError.notConnected

        do {
            _ = try await uploader(session: stubbedSession())
                .upload(data: Data("x".utf8), contentType: "image/png", signer: signer)
            XCTFail("expected the signer error to propagate")
        } catch {
            // expected — the kernel/remote signer was unavailable.
        }
        XCTAssertFalse(httpAttempted, "no HTTP request should be made when signing fails")
    }
}

// MARK: - Fake signer

/// `NostrSigner` test double. Records the last draft it was asked to sign and
/// returns a deterministic `SignedNostrEvent`, so tests can assert the upload
/// path forwards the kernel-signed event verbatim. Never touches crypto.
private final class FakeSigner: NostrSigner, @unchecked Sendable {
    var signError: Error?
    private(set) var lastDraft: NostrEventDraft?
    private(set) var lastSigned: SignedNostrEvent?

    func publicKey() async throws -> String { "fake-pubkey-hex" }

    func sign(_ draft: NostrEventDraft) async throws -> SignedNostrEvent {
        lastDraft = draft
        if let signError { throw signError }
        let signed = SignedNostrEvent(
            id: "deadbeef-id",
            pubkey: "fake-pubkey-hex",
            created_at: draft.createdAt,
            kind: draft.kind,
            tags: draft.tags,
            content: draft.content,
            sig: "fake-sig"
        )
        lastSigned = signed
        return signed
    }
}

// MARK: - Stub URLProtocol

/// In-memory HTTP interceptor. `handler` maps a request to a
/// `(statusCode, body, headers)` triple; no request leaves the process.
private final class StubURLProtocol: URLProtocol {
    /// Set per-test. Marked `nonisolated(unsafe)` because `URLProtocol` invokes
    /// it on its own loading thread; tests run serially and reset it in tearDown.
    nonisolated(unsafe) static var handler:
        ((URLRequest) -> (status: Int, body: Data, headers: [String: String]?))?

    /// `URLProtocol` strips the body from the stored `URLRequest`, exposing it
    /// only via `httpBodyStream`. Drain the stream to recover the bytes.
    static func bodyData(_ request: URLRequest) -> Data? {
        if let body = request.httpBody { return body }
        guard let stream = request.httpBodyStream else { return nil }
        stream.open()
        defer { stream.close() }
        var data = Data()
        let bufferSize = 4096
        var buffer = [UInt8](repeating: 0, count: bufferSize)
        while stream.hasBytesAvailable {
            let read = stream.read(&buffer, maxLength: bufferSize)
            if read <= 0 { break }
            data.append(buffer, count: read)
        }
        return data
    }

    override class func canInit(with request: URLRequest) -> Bool { handler != nil }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let handler = StubURLProtocol.handler, let url = request.url else {
            client?.urlProtocol(self, didFailWithError: URLError(.badServerResponse))
            return
        }
        let (status, body, headers) = handler(request)
        let response = HTTPURLResponse(
            url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: body)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}
