import Foundation

// MARK: - SyncCapabilityBridge
//
// Thread-safe adapter for the generated UniFFI capability callback. Runs on the
// Rust actor thread (a plain background thread), so it MUST NOT be @MainActor.
// Namespace ownership and capability dispatch live in `PodcastCapabilities`.

/// Synchronous capability adapter registered as the C capability callback.
///
/// Not `@MainActor` — the callback runs on the Rust actor thread. HTTP can run
/// directly here because `HttpCapability` is thread-safe; main-actor
/// capabilities are routed through `PodcastCapabilities` with an explicit main
/// hop inside that routing contract.
final class SyncCapabilityBridge: PodcastCapabilitySink, @unchecked Sendable {

    private let http: HttpCapability

    init(http: HttpCapability = PodcastCapabilityInstances.http) {
        self.http = http
        // `PodcastCapabilities.start()` is not part of the current launch
        // path, but the Rust callback can issue HTTP immediately after
        // registration. Start the canonical HTTP executor idempotently here so
        // unifying the instance does not regress early feed fetches.
        self.http.start()
    }

    /// Wire the async HTTP report sink onto the canonical `HttpCapability`
    /// instance — the one also held by `PodcastCapabilities.shared`. Called
    /// from `PodcastHandle.attachHttpReportChannel()` so the `URLSession`
    /// completion can push the result to `nmp_app_podcast_http_report`.
    func attachHttpReport(_ sink: @escaping (String) -> Void) {
        http.attach(sendReport: sink)
    }

    /// Route the raw kernel `CapabilityRequest` JSON and return a raw
    /// `CapabilityEnvelope` JSON. Never throws, never returns nil (D6).
    func handle(requestJSON: String) -> String {
        PodcastCapabilities.handleCapabilityCallbackJSON(requestJSON, http: http)
    }

    func onCapabilityRequest(requestJson: String) -> String {
        handle(requestJSON: requestJson)
    }
}
