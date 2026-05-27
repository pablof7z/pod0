import Foundation

// ─── dispatch_action return envelope ─────────────────────────────────────
//
// PR-A pattern: the Rust kernel returns `{"correlation_id":"<id>"}` on accept
// or `{"error":"<message>"}` on reject. Parsing here lets callers drive a
// spinner keyed on the correlation_id (or surface the rejection as a toast).

/// Synchronous outcome of `nmp_app_dispatch_action`.
enum DispatchResult: Equatable {
    /// The action was accepted and enqueued. Carries the `correlation_id`
    /// minted by `ActionRegistry::start`.
    case accepted(correlationId: String)
    /// The action was rejected synchronously. Carries a human-readable reason.
    case failure(_ message: String)

    var correlationId: String? {
        if case let .accepted(id) = self { return id }
        return nil
    }

    var errorMessage: String? {
        if case let .failure(msg) = self { return msg }
        return nil
    }

    /// Parse the JSON envelope returned by `nmp_app_dispatch_action`.
    ///
    /// Contract: every non-null app returns either
    /// `{"correlation_id":"<32-hex>"}` or `{"error":"<reason>"}`.
    /// Anything else degrades to `.failure` so no action is silently lost.
    static func parse(envelope: String) -> DispatchResult {
        guard let data = envelope.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return .failure(
                "dispatch envelope was not a JSON object (bytes=\(envelope.utf8.count))")
        }
        if let correlationId = object["correlation_id"] as? String, !correlationId.isEmpty {
            return .accepted(correlationId: correlationId)
        }
        if let message = object["error"] as? String {
            return .failure(message)
        }
        return .failure(
            "dispatch envelope missing both correlation_id and error (bytes=\(envelope.utf8.count))")
    }
}
