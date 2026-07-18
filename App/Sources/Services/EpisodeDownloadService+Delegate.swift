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
        let identity = EpisodeDownloadService.parseTaskDescription(downloadTask.taskDescription)
        let descID = identity?.episodeID ?? downloadTask.taskDescription.flatMap(UUID.init(uuidString:))
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
        let identity = EpisodeDownloadService.parseTaskDescription(downloadTask.taskDescription)
        let descID = identity?.episodeID
            ?? downloadTask.taskDescription.flatMap(UUID.init(uuidString:))

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
                    message: "Could not save download.", classification: .corruptArtifact,
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
            guard let jobID = service.taskIDToJobID[taskID] ?? identity?.jobID,
                  let inputVersion = service.taskIDToInputVersion[taskID]
                    ?? identity?.inputVersion else {
                try? FileManager.default.removeItem(at: interim)
                service.handleFailure(
                    episodeID: episodeID,
                    message: "Download finished without durable attempt identity."
                )
                return
            }
            await service.handleFinished(
                episodeID: episodeID,
                jobID: jobID,
                inputVersion: inputVersion,
                interim: interim
            )
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
        let descID = EpisodeDownloadService.parseTaskDescription(
            task.taskDescription
        )?.episodeID ?? task.taskDescription.flatMap(UUID.init(uuidString:))
        let resumeData = nserr.userInfo[NSURLSessionDownloadTaskResumeData] as? Data
        let httpStatus = (task.response as? HTTPURLResponse)?.statusCode
        let requestURL = task.originalRequest?.url?.absoluteString
        let productFailure = ProductFailure.classify(error)
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
                message: productFailure.diagnosticSummary, classification: productFailure.code.jobErrorClass,
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

    /// Stages immutable attempt output. The workflow verifier promotes and
    /// selects it only while the attempt still owns the lease.
    func handleFinished(
        episodeID: UUID,
        jobID: UUID,
        inputVersion: String,
        interim: URL
    ) async {
        guard let store = appStore,
              let episode = store.episode(id: episodeID) else {
            try? FileManager.default.removeItem(at: interim)
            return
        }
        let staged: StagedDownloadOutput
        do {
            staged = try await ArtifactVerificationExecutor.shared.stageDownload(
                interim,
                episode: episode,
                jobID: jobID,
                inputVersion: inputVersion
            )
        } catch {
            logger.error("download staging failed: \(error, privacy: .public)")
            handleFailure(
                episodeID: episodeID,
                message: "Could not stage download for verification.", classification: .corruptArtifact,
                auditDetails: [
                    .init("Stage", "attempt staging"),
                    .init("Error", String(describing: error)),
                ]
            )
            return
        }
        EpisodeDownloadStore.shared.clearResumeData(for: episode)
        // Drop in-memory bookkeeping.
        if let task = episodeIDToTask[episodeID] {
            taskIDToEpisodeID[task.taskIdentifier] = nil
            taskIDToJobID[task.taskIdentifier] = nil
            taskIDToInputVersion[task.taskIdentifier] = nil
        }
        episodeIDToTask[episodeID] = nil
        clearProgress(for: episodeID)
        EpisodeAuditLogStore.shared.record(
            episodeID: episodeID,
            kind: .downloadFinished,
            severity: .success,
            summary: "Downloaded \(Self.formatBytes(staged.byteCount)); verifying",
            details: [
                .init("Bytes", String(staged.byteCount)),
                .init("File", staged.fileURL.lastPathComponent),
                .init("URL", episode.enclosureURL.absoluteString),
            ]
        )
        logger.info(
            "download staged for \(episodeID, privacy: .public) (\(staged.byteCount, privacy: .public) bytes)"
        )
        finishWaiter(episodeID: episodeID, result: .success(staged.fileURL))
        WorkflowRuntime.shared.wake()
    }

    /// Pushes the terminal `.failed` state. Caller has already squirreled
    /// resume data away if any was attached to the error. Extra audit detail
    /// (HTTP status, error domain + code) is captured into the audit log so
    /// the Diagnostics sheet can show *why* a download failed.
    func handleFailure(
        episodeID: UUID,
        message: String, classification: JobErrorClass = .unexpected,
        auditDetails: [EpisodeAuditEvent.Detail] = []
    ) {
        guard appStore != nil else { return }
        if let task = episodeIDToTask[episodeID] {
            taskIDToEpisodeID[task.taskIdentifier] = nil
            taskIDToJobID[task.taskIdentifier] = nil
            taskIDToInputVersion[task.taskIdentifier] = nil
        }
        episodeIDToTask[episodeID] = nil
        clearProgress(for: episodeID)
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
        finishWaiter(
            episodeID: episodeID,
            result: .failure(JobFailure(classification: classification, message: message))
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
