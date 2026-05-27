import Foundation

// MARK: - DownloadCapability envelope encoding + task rehydration
//
// Helpers split out of `DownloadCapability.swift` to keep that file
// under the 300-LOC soft limit (AGENTS.md). Two concerns live here:
//
//   * **`CapabilityEnvelope` encoding** — the `{ok,error}Envelope`
//     helpers that wrap a result payload in the kernel-side
//     `CapabilityEnvelope` shape. Used by `handleJSON(_:)`.
//   * **Background-relaunch task rehydration** — the `getAllTasks`
//     pump that rebuilds the `episode_id → task` map from the
//     persistent background session.

extension DownloadCapability {

    // MARK: - Background-relaunch task rehydration

    /// Rebuild the `episode_id → task` map from the active background
    /// session. The session retains its tasks across launches; without
    /// this rehydration, an OS-replayed delegate callback would still
    /// recover the episode id from `taskDescription` (so the *report*
    /// half is safe), but a `PauseDownload` / `CancelDownload` arriving
    /// before any delegate callback would find an empty map and become
    /// a no-op.
    ///
    /// Marked `internal` so the executor's `init` can invoke it; it is
    /// only ever called from the same actor.
    func rehydrateExistingTasks() {
        session.getAllTasks { [weak self] tasks in
            Task { @MainActor [weak self] in
                guard let self else { return }
                for task in tasks {
                    guard let downloadTask = task as? URLSessionDownloadTask,
                          let episodeID = task.taskDescription,
                          !episodeID.isEmpty
                    else { continue }
                    if self.taskByEpisode[episodeID] == nil {
                        self.taskByEpisode[episodeID] = downloadTask
                    }
                }
            }
        }
    }

    // MARK: - Envelope encoding

    func okEnvelope(correlationID: String) -> String {
        let env = CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: correlationID,
            resultJSON: "{\"status\":\"ok\"}")
        return Self.encodeEnvelope(env) ?? "{}"
    }

    func errorEnvelope(correlationID: String, message: String) -> String {
        let payload = "{\"status\":\"error\",\"message\":\"\(message)\"}"
        let env = CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: correlationID,
            resultJSON: payload)
        return Self.encodeEnvelope(env) ?? "{}"
    }

    static func encodeEnvelope<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
