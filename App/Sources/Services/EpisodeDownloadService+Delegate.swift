import Foundation
import os.log

// MARK: - DownloadCoordinator

/// `URLSessionDownloadDelegate` adapter for `EpisodeDownloadService`.
///
/// Lives outside the `@MainActor` island so it can satisfy `URLSession`'s
/// `NSObjectProtocol` delegate contract (callbacks are nonisolated and may
/// land on an arbitrary queue, including after a background-relaunch). Every
/// callback hops back to the main actor before touching the service.
///
/// `weak service` keeps us from extending the service singleton's lifetime
/// past the process; in practice the singleton lives forever, so this is a
/// belt-and-braces choice.
final class DownloadCoordinator: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {

    // MARK: State

    /// Lock-guarded weak reference. The service is created on the main actor
    /// and lives forever, so the weakness is largely belt-and-braces.
    private let lock = NSLock()
    private weak var _service: EpisodeDownloadService?
    private static let logger = Logger.app("DownloadCoordinator")

    var service: EpisodeDownloadService? {
        lock.lock(); defer { lock.unlock() }
        return _service
    }

    // MARK: Init

    /// Two-step construction: the service needs the coordinator to make its
    /// `URLSession`, but the coordinator needs a back-reference to the service
    /// so it can dispatch onto the main actor. We init the coordinator first
    /// without a service, then `bind(service:)` after the session is built.
    override init() {
        super.init()
    }

    func bind(service: EpisodeDownloadService) {
        lock.lock(); defer { lock.unlock() }
        self._service = service
    }

    // MARK: - URLSessionDownloadDelegate

    /// Called as bytes arrive. Hops to the main actor and lets the service
    /// throttle the publish.
    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        let taskID = downloadTask.taskIdentifier
        let descID = downloadTask.taskDescription.flatMap(UUID.init(uuidString:))
        let expected = totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : nil
        let written = totalBytesWritten
        Task { @MainActor [weak service] in
            guard let service else { return }
            let episodeID = service.taskIDToEpisodeID[taskID] ?? descID
            guard let episodeID else { return }
            // Re-attach descID-discovered ID so subsequent ticks short-circuit
            // the lookup.
            service.taskIDToEpisodeID[taskID] = episodeID
            service.handleProgress(
                episodeID: episodeID,
                totalBytesWritten: written,
                totalBytesExpectedToWrite: expected
            )
        }
    }

    /// Called when the download lands on disk. The temp `location` is valid
    /// only inside this callback — we must move it synchronously *before*
    /// dispatching back to the main actor, otherwise the file is gone.
    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didFinishDownloadingTo location: URL
    ) {
        let taskID = downloadTask.taskIdentifier
        let descID = downloadTask.taskDescription.flatMap(UUID.init(uuidString:))

        // Move the file synchronously here. We don't yet know the destination
        // because we need the Episode for the extension — so move into a temp
        // subdirectory first, then let the main-actor handler shuttle into
        // its final spot. This is much safer than crossing actors with a
        // tempfile that could vanish.
        let interim: URL
        do {
            let dir = FileManager.default.temporaryDirectory
                .appendingPathComponent("podcastr-downloads-staging", isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            interim = dir.appendingPathComponent("\(taskID)-\(UUID().uuidString)")
            // Remove any stale file at the interim path (defensive).
            if FileManager.default.fileExists(atPath: interim.path) {
                try FileManager.default.removeItem(at: interim)
            }
            try FileManager.default.moveItem(at: location, to: interim)
        } catch {
            Self.logger.error("staging move failed: \(error, privacy: .public)")
            let errorString = String(describing: error)
            Task { @MainActor [weak service] in
                guard let service else { return }
                let episodeID = service.taskIDToEpisodeID[taskID] ?? descID
                guard let episodeID else { return }
                service.handleFailure(
                    episodeID: episodeID,
                    message: "Could not save download.",
                    auditDetails: [
                        .init("Stage", "staging move"),
                        .init("Error", errorString),
                    ]
                )
            }
            return
        }

        Task { @MainActor [weak service] in
            guard let service else { return }
            let episodeID = service.taskIDToEpisodeID[taskID] ?? descID
            guard let episodeID else {
                try? FileManager.default.removeItem(at: interim)
                return
            }
            service.handleFinished(episodeID: episodeID, interim: interim)
        }
    }

    /// Generic completion (only fires for failures or graceful end-of-task).
    /// The success path lands first in `didFinishDownloadingTo` above; this
    /// hook is the place to surface errors.
    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        guard let error else { return }
        let nserr = error as NSError
        // .cancelled is the user-initiated path — handled in cancel().
        if nserr.domain == NSURLErrorDomain, nserr.code == NSURLErrorCancelled {
            return
        }
        let taskID = task.taskIdentifier
        let descID = task.taskDescription.flatMap(UUID.init(uuidString:))
        let resumeData = nserr.userInfo[NSURLSessionDownloadTaskResumeData] as? Data
        let httpStatus = (task.response as? HTTPURLResponse)?.statusCode
        let requestURL = task.originalRequest?.url?.absoluteString
        let errorDescription = error.localizedDescription
        let errorDomain = nserr.domain
        let errorCode = nserr.code
        Task { @MainActor [weak service] in
            guard let service else { return }
            let episodeID = service.taskIDToEpisodeID[taskID] ?? descID
            guard let episodeID else { return }
            if let resumeData,
               let store = service.appStore,
               let episode = store.episode(id: episodeID) {
                EpisodeDownloadStore.shared.writeResumeData(resumeData, for: episode)
            }
            var details: [EpisodeAuditEvent.Detail] = [
                .init("Error domain", errorDomain),
                .init("Error code", String(errorCode)),
            ]
            if let httpStatus { details.append(.init("HTTP status", String(httpStatus))) }
            if let requestURL { details.append(.init("URL", requestURL)) }
            if resumeData != nil { details.append(.init("Resume data saved", "yes")) }
            service.handleFailure(
                episodeID: episodeID,
                message: errorDescription,
                auditDetails: details
            )
        }
    }

    func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        Task { @MainActor [weak service] in
            service?.handleBackgroundEventsFinished(for: session)
        }
    }
}

// MARK: - EpisodeDownloadService progress handlers

extension EpisodeDownloadService {

    /// Throttled progress publish. Updates the `@Observable` `progress`
    /// dictionary on every tick (cheap) but only writes the coarse-grained
    /// store mutation when neither the 5%-jump nor 200 ms-elapsed gate has
    /// fired (the store mutation is intentionally rare).
    func handleProgress(
        episodeID: UUID,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64?
    ) {
        let fraction: Double
        if let expected = totalBytesExpectedToWrite, expected > 0 {
            fraction = max(0, min(1, Double(totalBytesWritten) / Double(expected)))
        } else {
            fraction = 0
        }
        progress[episodeID] = fraction
        if let expected = totalBytesExpectedToWrite { expectedBytes[episodeID] = expected }

        let now = Date()
        let lastFraction = lastPublishedProgress[episodeID] ?? 0
        let lastDate = lastPublishedAt[episodeID] ?? .distantPast
        let bigJump = fraction - lastFraction >= 0.05
        let stale = now.timeIntervalSince(lastDate) >= 0.2
        guard bigJump || stale else { return }
        lastPublishedProgress[episodeID] = fraction
        lastPublishedAt[episodeID] = now
        // Don't write to AppStateStore on every tick — that would thrash
        // Persistence + Spotlight + Widgets. Progress lives on this service.
        // We only touch the store on terminal events. (See class doc.)
    }

    /// Moves `interim` into the canonical store location and pushes the
    /// terminal `.downloaded` state to `AppStateStore`.
    func handleFinished(episodeID: UUID, interim: URL) {
        guard let store = appStore,
              let episode = store.episode(id: episodeID) else {
            try? FileManager.default.removeItem(at: interim)
            return
        }
        let destination = EpisodeDownloadStore.shared.localFileURL(for: episode)
        let fm = FileManager.default
        do {
            // Defensive: clear any pre-existing file at destination.
            if fm.fileExists(atPath: destination.path) {
                try fm.removeItem(at: destination)
            }
            try fm.moveItem(at: interim, to: destination)
        } catch {
            logger.error("move-to-final failed: \(error, privacy: .public)")
            handleFailure(
                episodeID: episodeID,
                message: "Could not save download to library.",
                auditDetails: [
                    .init("Stage", "move to final destination"),
                    .init("Destination", destination.lastPathComponent),
                    .init("Error", String(describing: error)),
                ]
            )
            return
        }
        EpisodeDownloadStore.shared.clearResumeData(for: episode)
        let size = EpisodeDownloadStore.shared.byteCount(for: episode) ?? 0
        // Drop in-memory bookkeeping.
        if let task = episodeIDToTask[episodeID] {
            taskIDToEpisodeID[task.taskIdentifier] = nil
        }
        episodeIDToTask[episodeID] = nil
        clearProgress(for: episodeID)
        store.setEpisodeDownloadState(
            episodeID,
            state: .downloaded(localFileURL: destination, byteCount: size)
        )
        EpisodeAuditLogStore.shared.record(
            episodeID: episodeID,
            kind: .downloadFinished,
            severity: .success,
            summary: "Downloaded \(Self.formatBytes(size))",
            details: [
                .init("Bytes", String(size)),
                .init("File", destination.lastPathComponent),
                .init("URL", episode.enclosureURL.absoluteString),
            ]
        )
        logger.info(
            "download finished for \(episodeID, privacy: .public) (\(size, privacy: .public) bytes)"
        )
        // Transcription is data extraction, not a user action. The moment a
        // file lands on disk we kick the pipeline so the publisher transcript
        // (or the user's configured STT provider) fills the transcript before
        // they ever ask. The service is idempotent (`inFlight` dedup) and
        // gated on per-category opt-out + STT settings/key resolution, so
        // a double-fire here from any other entry point is harmless.
        Task { @MainActor in
            await TranscriptIngestService.shared.ingest(episodeID: episodeID)
        }
    }

    /// Pushes the terminal `.failed` state. Caller has already squirreled
    /// resume data away if any was attached to the error. Extra audit detail
    /// (HTTP status, error domain + code) is captured into the audit log so
    /// the Diagnostics sheet can show *why* a download failed.
    func handleFailure(
        episodeID: UUID,
        message: String,
        auditDetails: [EpisodeAuditEvent.Detail] = []
    ) {
        guard let store = appStore else { return }
        if let task = episodeIDToTask[episodeID] {
            taskIDToEpisodeID[task.taskIdentifier] = nil
        }
        episodeIDToTask[episodeID] = nil
        clearProgress(for: episodeID)
        store.setEpisodeDownloadState(episodeID, state: .failed(message: message))
        EpisodeAuditLogStore.shared.record(
            episodeID: episodeID,
            kind: .downloadFailed,
            severity: .failure,
            summary: message,
            details: auditDetails
        )
        logger.notice(
            "download failed for \(episodeID, privacy: .public): \(message, privacy: .public)"
        )
    }

    /// Pretty-prints a byte count. Kept on the service so both the delegate
    /// and any future caller can use the same units.
    static func formatBytes(_ bytes: Int64) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: bytes)
    }
}
