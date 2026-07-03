import Foundation

// ─── Signed-events registry (sign-for-return resolver) ────────────────────

/// Thread-safe find-or-register resolver for `signed_events` projection
/// results. Every kernel frame's `projections["signed_events"]` map is
/// `ingest`ed here under a lock; `awaitResult(correlationID:)` either consumes
/// an already-buffered result or installs a continuation the next `ingest`
/// resolves. This is the structural guarantee that the drain-once frame is
/// never missed between the synchronous `PodcastApp.signEventForReturn` return
/// and the caller's `await`.
final class SignedEventsRegistry: @unchecked Sendable {
    private let lock = NSLock()
    /// Results that drained before a waiter registered. Keyed by correlation id.
    private var buffered: [String: Result<String, Error>] = [:]
    /// Waiters that registered before their result drained.
    private var waiters: [String: CheckedContinuation<String, Error>] = [:]

    /// Ingest one frame's `signed_events` projection. Each value is
    /// `{ "ok": true, "signed_json": "…" }` or `{ "ok": false, "error": "…" }`.
    /// Resolves any registered waiter immediately; otherwise buffers the result.
    func ingest(envelopePayload data: Data) {
        guard
            let raw = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let value = raw["v"] as? [String: Any],
            let projections = value["projections"] as? [String: Any],
            let signed = projections["signed_events"] as? [String: Any],
            !signed.isEmpty
        else { return }

        var resolved: [(CheckedContinuation<String, Error>, Result<String, Error>)] = []
        lock.lock()
        for (correlationID, entry) in signed {
            guard let object = entry as? [String: Any] else { continue }
            let result: Result<String, Error>
            if let ok = object["ok"] as? Bool, ok, let signedJSON = object["signed_json"] as? String {
                result = .success(signedJSON)
            } else {
                let message = (object["error"] as? String) ?? "kernel signing failed"
                result = .failure(NostrSignerError.remoteRejected(message))
            }
            if let waiter = waiters.removeValue(forKey: correlationID) {
                resolved.append((waiter, result))
            } else {
                buffered[correlationID] = result
            }
        }
        lock.unlock()
        // Resume continuations outside the lock.
        for (waiter, result) in resolved { waiter.resume(with: result) }
    }

    /// Await the signed-event JSON for `correlationID`. Returns the flat NIP-01
    /// event JSON on success; throws on a kernel-reported error.
    func awaitResult(correlationID: String) async throws -> String {
        try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            if let buffered = buffered.removeValue(forKey: correlationID) {
                lock.unlock()
                continuation.resume(with: buffered)
                return
            }
            waiters[correlationID] = continuation
            lock.unlock()
        }
    }

    /// Fail an outstanding waiter for `correlationID` with `error` and stop
    /// retaining it. No-op if the result already drained (the waiter is gone).
    /// Used by the caller-owned timeout so a kernel that never resolves the id
    /// (e.g. a null/unstarted app — the NMP contract says "the caller's
    /// continuation times out") surfaces as a thrown error, not a permanent
    /// hang. Also drops any buffered-but-unclaimed result for the id so it
    /// cannot leak.
    func cancel(correlationID: String, with error: Error) {
        lock.lock()
        let waiter = waiters.removeValue(forKey: correlationID)
        buffered.removeValue(forKey: correlationID)
        lock.unlock()
        waiter?.resume(throwing: error)
    }
}
