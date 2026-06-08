import Foundation

// MARK: - SyncCapabilityBridge
//
// Thread-safe capability router for the C capability callback registered via
// `nmp_app_set_capability_callback`. Runs on the Rust actor thread (a plain
// background thread), so it MUST NOT be @MainActor and MUST only call
// synchronous, thread-safe code.
//
// Currently routes `nmp.http.capability` requests to a dedicated
// `HttpCapability` instance. Other namespaces return error envelopes (D6 —
// errors are data). The router expands as more synchronous capabilities land.

// MARK: - Helpers (must be callable outside @MainActor)

private func makeErrorEnvelope(namespace: String, correlationID: String, message: String) -> String {
    let result = "{\"status\":\"error\",\"message\":\"\(message)\"}"
    let escaped = namespace.replacingOccurrences(of: "\"", with: "\\\"")
    let cidEscaped = correlationID.replacingOccurrences(of: "\"", with: "\\\"")
    return "{\"namespace\":\"\(escaped)\",\"correlation_id\":\"\(cidEscaped)\",\"result_json\":\"\(result.replacingOccurrences(of: "\"", with: "\\\""))\"}"
}

/// Synchronous capability router registered as the C capability callback.
///
/// Not `@MainActor` — the callback runs on the Rust actor thread. All
/// capabilities it delegates to MUST be thread-safe and synchronous (the
/// Rust socket blocks the actor thread until the callback returns).
final class SyncCapabilityBridge: @unchecked Sendable {

    private let http: HttpCapability

    init() {
        self.http = HttpCapability()
        self.http.start()
    }

    /// Wire the async HTTP report sink onto this bridge's `HttpCapability`
    /// instance — the one that actually serves `handleAsyncJSON` on the actor
    /// thread. Called from `PodcastHandle.attachHttpReportChannel()` so the
    /// `URLSession` completion can push the result to `nmp_app_podcast_http_report`.
    func attachHttpReport(_ sink: @escaping (String) -> Void) {
        http.attach(sendReport: sink)
    }

    /// Route the raw kernel `CapabilityRequest` JSON and return a raw
    /// `CapabilityEnvelope` JSON. Never throws, never returns nil (D6).
    func handle(requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let obj = try? JSONDecoder().decode(NamespaceProbe.self, from: data)
        else {
            return makeErrorEnvelope(
                namespace: "", correlationID: "", message: "malformed-request")
        }

        switch obj.namespace {
        case HttpCapability.namespace:
            return http.handleJSON(requestJSON)
        case HttpCapability.asyncNamespace:
            // Fire-and-forget: kicks off a non-blocking URLSession and returns
            // an immediate ack. The result is delivered out of band via the
            // report sink → `nmp_app_podcast_http_report`. Safe on the actor
            // thread because it never blocks.
            return http.handleAsyncJSON(requestJSON)
        case AudioCapability.namespace:
            // @MainActor; hop to main thread synchronously. Safe: the actor
            // thread is never the main thread, and the main thread is never
            // blocked waiting on the actor thread at this call site.
            return DispatchQueue.main.sync {
                MainActor.assumeIsolated {
                    PodcastCapabilities.shared.audio.handleJSON(requestJSON)
                }
            }
        case DownloadCapability.namespace:
            // Same main-actor hop as AudioCapability — DownloadCapability is
            // also @MainActor (URLSession delegate state lives there).
            return DispatchQueue.main.sync {
                MainActor.assumeIsolated {
                    PodcastCapabilities.shared.download.handleJSON(requestJSON)
                }
            }
        default:
            return makeErrorEnvelope(
                namespace: obj.namespace,
                correlationID: obj.correlation_id,
                message: "unknown-namespace")
        }
    }

    /// Minimal decode just to read the routing namespace + correlation_id
    /// without pulling the full payload. Snake_case field names match the
    /// Rust `CapabilityRequest` wire format verbatim.
    private struct NamespaceProbe: Decodable {
        let namespace: String
        let correlation_id: String
    }
}
