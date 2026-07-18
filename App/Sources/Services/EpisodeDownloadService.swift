import Foundation
import Network
import os.log

// MARK: - EpisodeDownloadService

/// Real implementation of the per-episode enclosure downloader.
///
/// Durable job lifecycle lives in JobStore. Episode state only projects a
/// verified selected file as `.downloaded`; progress remains in this service.
///
/// Persistence philosophy:
/// - **Coarse transitions** push to `AppStateStore` so a relaunch knows the
///   episode is downloaded. The store's `didSet` is heavy (Persistence,
///   Spotlight, WidgetKit, iCloud KV) so we publish at most three writes per
///   download (start / terminal). Resume data also reaches the store as
///   `.downloading(0, nil)` so the row keeps its capsule.
/// - **Fine progress** lives on this service's own `@Observable` `progress`
///   dictionary and is throttled to 5% / 200 ms — UI binds to it directly.
///
/// Concurrency: this type is `@MainActor`. The `URLSessionDownloadDelegate`
/// callbacks land on a private nonisolated coordinator (see
/// `EpisodeDownloadService+Delegate.swift`) which hops back here via a `Task`.
@MainActor
@Observable
final class EpisodeDownloadService {

    // MARK: Singleton

    static let shared = EpisodeDownloadService()

    // MARK: Logger

    let logger = Logger.app("EpisodeDownloadService")

    // MARK: Configuration

    /// Background URLSession identifier shared by the live session and the
    /// AppDelegate background-event handoff.
    static let backgroundSessionIdentifier = "io.f7z.podcast.downloads"

    // MARK: Observable surface

    /// Live progress per in-flight episode in `0...1`. Driven by the throttled
    /// delegate; consumers (`DownloadStatusCapsule`, the detail toolbar) read
    /// this directly to avoid hitting `AppStateStore.state` 5× per second.
    /// Setter is internal so the `+Delegate` extension (same module) can write.
    var progress: [UUID: Double] = [:]

    /// Approximate total bytes per in-flight episode (when the server reports
    /// `Content-Length`). `nil` until known.
    var expectedBytes: [UUID: Int64] = [:]

    // MARK: Internal state (also touched by the delegate extension)

    /// Maps the URLSession task identifier to the episode it's downloading.
    /// Lives on the main actor because the lookup happens on hop-back.
    var taskIDToEpisodeID: [Int: UUID] = [:]
    var taskIDToJobID: [Int: UUID] = [:]
    var taskIDToInputVersion: [Int: String] = [:]
    /// Inverse — used by `cancel(episodeID:)` to find the live task.
    var episodeIDToTask: [UUID: URLSessionDownloadTask] = [:]
    /// Last published progress value per episode — drives the 5% throttle.
    var lastPublishedProgress: [UUID: Double] = [:]
    /// Wall-clock of the last progress publish — drives the 200 ms throttle.
    var lastPublishedAt: [UUID: Date] = [:]
    var downloadWaiters: [UUID: [UUID: CheckedContinuation<Result<URL, Error>, Never>]] = [:]
    var terminalResults: [UUID: Result<URL, Error>] = [:]

    /// The store the service mutates. Wired from `RootView.onAppear` so the
    /// service stays a singleton without owning a strong reference at init time.
    weak var appStore: AppStateStore?

    // MARK: Network monitoring (Wi-Fi guard for AutoDownloadPolicy)

    private let pathMonitor = NWPathMonitor()
    private let pathQueue = DispatchQueue(label: "io.f7z.podcast.downloads.path")
    /// Wraps the cached Wi-Fi flag so we can mutate from a background queue
    /// (NWPathMonitor) without tangling with `@Observable`'s tracking.
    let pathState = PathState()

    // MARK: URLSession

    /// Background-aware session. Created once in `init` (the `@Observable`
    /// macro doesn't allow `lazy`) so the same session handles every download
    /// for the process and so the OS can replay delegate events on relaunch.
    let session: URLSession

    /// Strong reference to keep the delegate alive for the session's lifetime.
    let coordinator: DownloadCoordinator
    private var backgroundCompletionHandlers: [String: () -> Void] = [:]

    // MARK: Init

    init() {
        let coordinator = DownloadCoordinator()
        self.coordinator = coordinator
        let config = URLSessionConfiguration.background(withIdentifier: Self.backgroundSessionIdentifier)
        config.isDiscretionary = false
        config.sessionSendsLaunchEvents = true
        config.allowsCellularAccess = true
        config.waitsForConnectivity = true
        self.session = URLSession(configuration: config, delegate: coordinator, delegateQueue: nil)
        coordinator.bind(service: self)
        pathMonitor.pathUpdateHandler = { [pathState] path in
            let isWiFi = path.usesInterfaceType(.wifi)
            let status: DownloadNetworkStatus = if path.status != .satisfied {
                .unavailable
            } else if isWiFi {
                .wifi
            } else {
                .other
            }
            pathState.set(status)
            if path.status == .satisfied {
                Task { @MainActor in
                    EpisodeDownloadService.shared.resumeQueuedDownloadsIfPossible()
                    WorkflowRuntime.shared.dependencyChanged(for: .autoDownload)
                    WorkflowRuntime.shared.dependencyChanged(for: .download)
                }
            }
        }
        pathMonitor.start(queue: pathQueue)
    }

    /// Wires the service to the live store. Idempotent — safe to call from
    /// startup and from every UI surface that needs the service. Action
    /// surfaces still call it defensively so previews/tests with injected
    /// stores mutate the right state.
    func attach(appStore: AppStateStore) {
        self.appStore = appStore
        resumeQueuedDownloadsIfPossible()
    }

    func handleEventsForBackgroundURLSession(
        identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        guard identifier == Self.backgroundSessionIdentifier else {
            completionHandler()
            return
        }
        backgroundCompletionHandlers[identifier] = completionHandler
        logger.info("background URLSession handoff registered for \(identifier, privacy: .public)")
    }

    func handleBackgroundEventsFinished(for session: URLSession) {
        let identifier = session.configuration.identifier ?? Self.backgroundSessionIdentifier
        guard identifier == Self.backgroundSessionIdentifier else { return }
        guard let completionHandler = backgroundCompletionHandlers.removeValue(forKey: identifier) else {
            return
        }
        completionHandler()
        logger.info("background URLSession handoff completed for \(identifier, privacy: .public)")
    }

    // MARK: - Public API

    /// Persists user download intent. Only WorkCoordinator may admit the
    /// matching URLSession transfer.
    func download(episodeID: UUID) {
        WorkflowRuntime.shared.requestDownload(episodeID: episodeID, origin: .user)
    }

    func startAdmittedDownload(
        context: JobAttemptContext,
        jobStore: JobStore
    ) async throws -> String {
        let episodeID = context.job.subjectID
        guard let store = appStore,
              let episode = store.episode(id: episodeID) else {
            throw JobFailure(classification: .invalidInput, message: "Episode no longer exists")
        }
        if case .downloaded = episode.downloadState,
           EpisodeDownloadStore.shared.exists(for: episode),
           let data = try? Data(
               contentsOf: EpisodeDownloadStore.shared.localFileURL(for: episode),
               options: .mappedIfSafe
           ) {
            return ArtifactRepository.hash(data)
        }
        if let staged = EpisodeDownloadStore.shared.verifiedStagedOutput(
            episodeID: episodeID,
            jobID: context.job.id,
            inputVersion: context.job.inputVersion
        ) {
            return staged.contentHash
        }
        if episodeIDToTask[episodeID] == nil {
            terminalResults[episodeID] = nil
            try startTransfer(
                episodeID: episodeID,
                durableJobID: context.job.id,
                inputVersion: context.job.inputVersion,
                leaseToken: context.leaseToken,
                jobStore: jobStore
            )
        }
        let result = await waitForDownload(episodeID: episodeID, waiterID: context.job.id)
        switch result {
        case .success(let url):
            let data = try Data(contentsOf: url, options: .mappedIfSafe)
            return ArtifactRepository.hash(data)
        case .failure(let error as JobFailure):
            throw error
        case .failure(let error):
            throw JobFailure.classify(error)
        }
    }

    private func startTransfer(
        episodeID: UUID,
        durableJobID: UUID,
        inputVersion: String,
        leaseToken: UUID,
        jobStore: JobStore
    ) throws {
        guard let store = appStore,
              let episode = store.episode(id: episodeID) else {
            logger.error("download(\(episodeID, privacy: .public)) — store/episode missing")
            return
        }
        guard episodeIDToTask[episodeID] == nil else { return }

        let task: URLSessionDownloadTask
        let resumeData = EpisodeDownloadStore.shared.loadResumeData(for: episode)
        if let resumeData {
            task = session.downloadTask(withResumeData: resumeData)
        } else {
            task = session.downloadTask(with: episode.enclosureURL)
        }
        EpisodeAuditLogStore.shared.record(
            episodeID: episodeID,
            kind: .downloadRequested,
            severity: .info,
            summary: resumeData != nil ? "Resuming download" : "Starting download",
            details: [
                .init("URL", episode.enclosureURL.absoluteString),
                .init("MIME", episode.enclosureMimeType ?? "—"),
                .init("Resume data", resumeData.map { "\($0.count) bytes" } ?? "none"),
            ]
        )
        // taskDescription lets the coordinator recover the episode ID even
        // after the in-memory map is lost (e.g. background relaunch).
        task.taskDescription = Self.taskDescription(
            jobID: durableJobID,
            episodeID: episodeID,
            inputVersion: inputVersion
        )
        episodeIDToTask[episodeID] = task
        taskIDToEpisodeID[task.taskIdentifier] = episodeID
        taskIDToJobID[task.taskIdentifier] = durableJobID
        taskIDToInputVersion[task.taskIdentifier] = inputVersion
        progress[episodeID] = 0
        expectedBytes[episodeID] = nil
        lastPublishedProgress[episodeID] = 0
        lastPublishedAt[episodeID] = Date()

        try jobStore.recordExternalOperation(
            id: durableJobID,
            leaseToken: leaseToken,
            provider: "backgroundURLSession",
            externalID: String(task.taskIdentifier),
            state: "created"
        )

        task.resume()
        EpisodeAuditLogStore.shared.record(
            episodeID: episodeID,
            kind: .downloadStarted,
            severity: .info,
            summary: "URLSession task resumed",
            details: [
                .init("Task ID", String(task.taskIdentifier)),
                .init("Cellular allowed", "yes"),
            ]
        )
        logger.info("download started for \(episodeID, privacy: .public)")
    }

    private func waitForDownload(episodeID: UUID, waiterID: UUID) async -> Result<URL, Error> {
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

    nonisolated static func taskDescription(
        jobID: UUID,
        episodeID: UUID,
        inputVersion: String
    ) -> String {
        "job:\(jobID.uuidString):episode:\(episodeID.uuidString):input:\(inputVersion)"
    }

    nonisolated static func parseTaskDescription(
        _ value: String?
    ) -> (jobID: UUID, episodeID: UUID, inputVersion: String)? {
        guard let parts = value?.split(separator: ":"), parts.count == 6,
              parts[0] == "job", parts[2] == "episode", parts[4] == "input",
              let jobID = UUID(uuidString: String(parts[1])),
              let episodeID = UUID(uuidString: String(parts[3])) else { return nil }
        return (jobID, episodeID, String(parts[5]))
    }

    func reconcileBackgroundTransfers(jobStore: JobStore) async {
        let tasks = await session.allTasks.compactMap { $0 as? URLSessionDownloadTask }
        let jobs = (try? jobStore.allJobs()) ?? []
        let tasksByID = Dictionary(uniqueKeysWithValues: tasks.map { ($0.taskIdentifier, $0) })
        let facts = tasks.map { task -> BackgroundDownloadTaskFact in
            let identity = Self.parseTaskDescription(task.taskDescription)
            return BackgroundDownloadTaskFact(
                taskIdentifier: task.taskIdentifier,
                jobID: identity?.jobID,
                episodeID: identity?.episodeID
            )
        }
        for action in DownloadReconciliationPlanner().plan(tasks: facts, jobs: jobs) {
            switch action {
            case .attach(let taskID, let jobID, let episodeID):
                guard let task = tasksByID[taskID] else { continue }
                episodeIDToTask[episodeID] = task
                taskIDToEpisodeID[taskID] = episodeID
                taskIDToJobID[taskID] = jobID
                taskIDToInputVersion[taskID] = jobs.first { $0.id == jobID }?.inputVersion
                try? jobStore.requeueInterrupted(id: jobID)
            case .cancelOrphan(let taskID):
                tasksByID[taskID]?.cancel()
            case .requeueMissingTask(let jobID):
                try? jobStore.requeueInterrupted(id: jobID)
            }
        }
    }

    /// Cancels the in-flight download for `episodeID`. Persists resume data
    /// where the server supports it so a later `download(episodeID:)` can pick
    /// up from the byte we left off at.
    func cancel(episodeID: UUID) {
        WorkflowRuntime.shared.cancelDownload(episodeID: episodeID)
        guard let task = episodeIDToTask[episodeID] else { return }
        let store = appStore
        task.cancel { [weak self] resumeData in
            // The completion runs on a background queue; hop to MainActor.
            Task { @MainActor in
                guard let self else { return }
                if let resumeData,
                   let episode = store?.episode(id: episodeID) {
                    EpisodeDownloadStore.shared.writeResumeData(resumeData, for: episode)
                }
                self.clearProgress(for: episodeID)
                store?.setEpisodeDownloadState(episodeID, state: .notDownloaded)
            }
        }
        episodeIDToTask[episodeID] = nil
        let removedTaskIDs = taskIDToEpisodeID.compactMap { $0.value == episodeID ? $0.key : nil }
        taskIDToEpisodeID = taskIDToEpisodeID.filter { $0.value != episodeID }
        for taskID in removedTaskIDs {
            taskIDToJobID[taskID] = nil
            taskIDToInputVersion[taskID] = nil
        }
        finishWaiter(
            episodeID: episodeID,
            result: .failure(JobFailure(
                classification: .cancelled,
                message: "Download cancelled by user."
            ))
        )
        EpisodeAuditLogStore.shared.record(
            episodeID: episodeID,
            kind: .downloadCancelled,
            severity: .warning,
            summary: "Download cancelled by user"
        )
        logger.info("download cancelled for \(episodeID, privacy: .public)")
    }

    /// Removes a downloaded file and resets state to `.notDownloaded`. Safe to
    /// call when no file exists.
    func delete(episodeID: UUID) {
        guard let store = appStore else { return }
        // Cancel anything in flight so we don't leave a zombie task.
        if episodeIDToTask[episodeID] != nil {
            cancel(episodeID: episodeID)
        }
        guard let episode = store.episode(id: episodeID) else { return }
        do {
            try EpisodeDownloadStore.shared.delete(for: episode)
        } catch {
            logger.error("delete failed for \(episodeID, privacy: .public): \(error, privacy: .public)")
        }
        clearProgress(for: episodeID)
        store.setEpisodeDownloadState(episodeID, state: .notDownloaded)
        WorkflowRuntime.shared.dependencyChanged(for: .download)
        EpisodeAuditLogStore.shared.record(
            episodeID: episodeID,
            kind: .downloadDeleted,
            severity: .info,
            summary: "Local file removed"
        )
    }

    // MARK: - Internal helpers (also called from the delegate extension)

    func clearProgress(for episodeID: UUID) {
        progress[episodeID] = nil
        expectedBytes[episodeID] = nil
        lastPublishedProgress[episodeID] = nil
        lastPublishedAt[episodeID] = nil
    }
}
