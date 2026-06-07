import Foundation
import os.log

// MARK: - DownloadCoordinator
//
// `URLSessionDownloadDelegate` adapter for `DownloadCapability`.
//
// Lives outside the `@MainActor` island so it can satisfy `URLSession`'s
// `NSObjectProtocol` delegate contract (callbacks are nonisolated and may
// land on an arbitrary queue, including after a background-relaunch).
// Every callback hops back to the main actor before touching the
// capability's state.

final class NmpDownloadCoordinator: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {

    // MARK: State

    /// Lock-guarded weak reference. The capability is created on the
    /// main actor and lives for the app's lifetime, so the weakness is
    /// largely belt-and-braces.
    private let lock = NSLock()
    private weak var _capability: DownloadCapability?
    private static let logger = Logger(subsystem: "io.f7z.podcast", category: "DownloadCoordinator")

    var capability: DownloadCapability? {
        lock.lock(); defer { lock.unlock() }
        return _capability
    }

    override init() {
        super.init()
    }

    func bind(capability: DownloadCapability) {
        lock.lock(); defer { lock.unlock() }
        self._capability = capability
    }

    // MARK: - URLSessionDownloadDelegate

    /// Bytes-arriving tick. Hops to the main actor and lets the capability
    /// apply the D8 throttle gate.
    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        guard let taskDesc = downloadTask.taskDescription, !taskDesc.isEmpty else {
            return
        }
        let (episodeID, _) = DownloadCapability.decodeTaskDescription(taskDesc)
        let total: UInt64? = totalBytesExpectedToWrite > 0
            ? UInt64(totalBytesExpectedToWrite)
            : nil
        let written = UInt64(max(0, totalBytesWritten))
        Task { @MainActor [weak capability] in
            guard let capability else { return }
            // Re-attach the live task into the map if a background
            // relaunch tick arrived before `rehydrateExistingTasks`
            // completed.
            if capability.taskByEpisode[episodeID] == nil {
                capability.taskByEpisode[episodeID] = downloadTask
            }
            capability.emitProgressIfThresholdCrossed(
                episodeID: episodeID,
                bytesDownloaded: written,
                totalBytes: total)
        }
    }

    /// The download landed on disk. The temp `location` is valid only
    /// inside this callback — move the file *synchronously* before
    /// hopping to the main actor, otherwise the temp file is gone by
    /// the time we get there.
    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didFinishDownloadingTo location: URL
    ) {
        guard let taskDesc = downloadTask.taskDescription, !taskDesc.isEmpty else {
            Self.logger.error("didFinishDownloadingTo: missing taskDescription")
            return
        }
        let (episodeID, kind) = DownloadCapability.decodeTaskDescription(taskDesc)
        let sourceURL = downloadTask.originalRequest?.url
        // Move synchronously off the delegate queue — `tempLocation` is
        // ephemeral. `kind` routes the file to the right directory.
        let moved = DownloadCapability.moveFinishedDownload(
            from: location,
            episodeID: episodeID,
            sourceURL: sourceURL,
            kind: kind)

        Task { @MainActor [weak capability] in
            guard let capability else { return }
            capability.taskByEpisode[episodeID] = nil
            capability.lastEmittedBytes[episodeID] = nil
            capability.lastEmittedAt[episodeID] = nil
            if let moved {
                capability.emit(.completed(
                    episodeID: episodeID,
                    localPath: moved.path))
                // Post-download re-entry into the transcript pipeline (the hook
                // `TranscriptIngestService.runAITranscription` documents): now
                // that the audio file has landed, kick transcription so
                // download → transcript → chapters/ads identification chains
                // automatically. Episodes only — local-model downloads have no
                // transcript. `ingest()` self-gates (in-flight dedup,
                // already-ready skip, category opt-out, kernel-resolved
                // provider), so it is safe to fire on every successful episode
                // download.
                if kind == .episode, let uuid = UUID(uuidString: episodeID) {
                    Task { await TranscriptIngestService.shared.ingest(episodeID: uuid) }
                }
            } else {
                capability.emit(.failed(
                    episodeID: episodeID,
                    error: "move-to-final-failed"))
            }
        }
    }

    /// Generic completion. Fires for failures and graceful end-of-task;
    /// the success path lands first in `didFinishDownloadingTo` above,
    /// so by the time we get here the task either failed or was
    /// cancelled. User-initiated `.cancelled` is handled by the
    /// `cancelDownload` command path; pause-with-resume hands a
    /// `NSURLErrorCancelled` here too, but the resume data is stashed
    /// by the cancel-handler completion and we don't re-emit a report.
    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        guard let error else { return }
        let nserr = error as NSError
        let isCancelled = nserr.domain == NSURLErrorDomain
            && nserr.code == NSURLErrorCancelled
        let resumeData = nserr.userInfo[NSURLSessionDownloadTaskResumeData] as? Data
        guard let taskDesc = task.taskDescription, !taskDesc.isEmpty else { return }
        let (episodeID, kind) = DownloadCapability.decodeTaskDescription(taskDesc)
        let errorDescription = error.localizedDescription

        Task { @MainActor [weak capability] in
            guard let capability else { return }
            if isCancelled {
                // Either:
                //   * a `CancelDownload` command — already emitted
                //     `Cancelled` and cleared resume data;
                //   * a `PauseDownload` command — the cancel handler
                //     stashed resume data and emitted `Paused`;
                //   * an OS-initiated cancel (unlikely with our
                //     configuration).
                // If resume data showed up here but wasn't already
                // persisted (e.g. an OS-driven cancel), stash it so a
                // subsequent `ResumeDownload` can use it.
                if let resumeData {
                    DownloadCapability.writeResumeData(resumeData, for: episodeID, kind: kind)
                }
                // The map entry was cleared by the command path; if it
                // wasn't (OS-driven cancel), drop it now.
                capability.taskByEpisode[episodeID] = nil
                return
            }
            capability.taskByEpisode[episodeID] = nil
            capability.lastEmittedBytes[episodeID] = nil
            capability.lastEmittedAt[episodeID] = nil
            // Persist resume data attached to the error so the next
            // `ResumeDownload` can pick up where this transport-layer
            // failure left off. D7 — we don't decide *whether* to
            // retry; we just keep the resume blob available.
            if let resumeData {
                DownloadCapability.writeResumeData(resumeData, for: episodeID, kind: kind)
            }
            capability.emit(.failed(
                episodeID: episodeID,
                error: errorDescription))
        }
    }

    /// All accrued background events have been replayed — invoke the
    /// stored OS completion handler so the system knows we're done.
    func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        Task { @MainActor [weak capability] in
            capability?.finishBackgroundEvents(for: session)
        }
    }
}
