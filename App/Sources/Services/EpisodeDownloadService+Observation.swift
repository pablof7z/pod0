import Foundation

@MainActor
extension EpisodeDownloadService {
    /// Event-driven observation for native consumers that need the raw local
    /// capability result. Durable admission and retry policy remain in the
    /// workflow; this only awaits the terminal URLSession observation.
    func observeDownloadCompletion(episodeID: UUID, observerID: UUID) async throws -> URL {
        try await waitForDownload(episodeID: episodeID, waiterID: observerID).get()
    }

    func waitForDownload(
        episodeID: UUID,
        waiterID: UUID
    ) async -> Result<URL, Error> {
        if let terminal = terminalResults[episodeID] { return terminal }
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                if let terminal = terminalResults[episodeID] {
                    continuation.resume(returning: terminal)
                } else {
                    downloadWaiters[episodeID, default: [:]][waiterID] = continuation
                }
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.finishWaiter(
                    episodeID: episodeID,
                    waiterID: waiterID,
                    result: .failure(JobFailure(
                        classification: .cancelled,
                        message: "Local download observation was cancelled."
                    ))
                )
            }
        }
    }

    func finishWaiter(
        episodeID: UUID,
        waiterID: UUID? = nil,
        result: Result<URL, Error>
    ) {
        if let waiterID {
            downloadWaiters[episodeID]?.removeValue(forKey: waiterID)?.resume(returning: result)
            if downloadWaiters[episodeID]?.isEmpty == true { downloadWaiters[episodeID] = nil }
            return
        }
        terminalResults[episodeID] = result
        let waiters = downloadWaiters.removeValue(forKey: episodeID)
        waiters?.values.forEach { $0.resume(returning: result) }
    }
}
