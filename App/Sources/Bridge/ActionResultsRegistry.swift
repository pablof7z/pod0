import Foundation

// MARK: - ActionResultsRegistry
//
// Thread-safe find-or-register resolver for `action_results` projection
// entries. Every kernel frame's `projections["action_results"]` array is
// `ingest`ed here under a lock; `awaitResult(correlationID:)` either
// consumes an already-buffered result or installs a continuation the next
// `ingest` resolves.
//
// Wire shape (from Rust `action_results_fb.rs`):
//   [
//     { "correlation_id": "…", "status": "published", "result": "…json…" },
//     { "correlation_id": "…", "status": "failed",    "error":  "…"      }
//   ]
//
// Used by `BlossomKernelUploader` to await the `BlobDescriptor` URL after
// dispatching `nmp.blossom.upload`. The pattern is structurally identical to
// `SignedEventsRegistry` (drain-once, race-free, kernel-settled).

/// An individual settled action result as decoded from the wire frame.
struct ActionResultEntry {
    let correlationID: String
    let status: String
    let error: String?
    /// The raw JSON string of the `result` field (e.g. a serialised
    /// `BlobDescriptor`). `nil` when the action failed.
    let resultJSON: String?
}

/// Thread-safe find-or-register resolver for `action_results` projection
/// entries. Each kernel frame that carries `projections["action_results"]` is
/// `ingest`ed; entries keyed by `correlation_id` are buffered or immediately
/// hand-off to a registered waiter.
final class ActionResultsRegistry: @unchecked Sendable {
    private let lock = NSLock()
    /// Entries that drained before a waiter registered. Keyed by correlation_id.
    private var buffered: [String: Result<ActionResultEntry, Error>] = [:]
    /// Waiters registered before their entry drained.
    private var waiters: [String: CheckedContinuation<ActionResultEntry, Error>] = [:]

    /// Ingest one frame's `action_results` projection array.
    /// Each element is `{ "correlation_id", "status", "result"?, "error"? }`.
    /// Resolves any registered waiter immediately; otherwise buffers the entry.
    func ingest(envelopePayload data: Data) {
        guard
            let raw = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let value = raw["v"] as? [String: Any],
            let projections = value["projections"] as? [String: Any],
            let results = projections["action_results"] as? [[String: Any]],
            !results.isEmpty
        else { return }

        var resolved: [(CheckedContinuation<ActionResultEntry, Error>, Result<ActionResultEntry, Error>)] = []
        lock.lock()
        for row in results {
            guard let cid = row["correlation_id"] as? String, !cid.isEmpty else { continue }
            let status = row["status"] as? String ?? ""
            let entry = ActionResultEntry(
                correlationID: cid,
                status: status,
                error: row["error"] as? String,
                resultJSON: row["result"] as? String)
            let result: Result<ActionResultEntry, Error>
            if let error = entry.error, !error.isEmpty, status != "published" {
                result = .failure(BlossomUploadError.serverRejected(error))
            } else {
                result = .success(entry)
            }
            if let waiter = waiters.removeValue(forKey: cid) {
                resolved.append((waiter, result))
            } else {
                buffered[cid] = result
            }
        }
        lock.unlock()
        for (waiter, result) in resolved { waiter.resume(with: result) }
    }

    /// Await the settled `ActionResultEntry` for `correlationID`. Throws if the
    /// action failed in the kernel-settled action result.
    func awaitResult(correlationID: String) async throws -> ActionResultEntry {
        try Task.checkCancellation()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                lock.lock()
                if Task.isCancelled {
                    lock.unlock()
                    continuation.resume(throwing: CancellationError())
                    return
                }
                if let buffered = buffered.removeValue(forKey: correlationID) {
                    lock.unlock()
                    continuation.resume(with: buffered)
                    return
                }
                waiters[correlationID] = continuation
                lock.unlock()
            }
        } onCancel: {
            cancelWaiter(correlationID: correlationID)
        }
    }

    /// Drop a Swift task-local waiter when the awaiting task is cancelled.
    /// This is lifecycle cleanup, not action failure policy; settled kernel
    /// failures still arrive through `action_results`.
    private func cancelWaiter(correlationID: String) {
        lock.lock()
        let waiter = waiters.removeValue(forKey: correlationID)
        lock.unlock()
        waiter?.resume(throwing: CancellationError())
    }
}
