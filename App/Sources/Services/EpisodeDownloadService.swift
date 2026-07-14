import Foundation
import Network
import os.log

// MARK: - EpisodeDownloadService

/// Real implementation of the per-episode enclosure downloader.
///
/// Lifecycle for a single episode:
/// `.notDownloaded` → `.downloading(progress, bytes)` → `.downloaded(URL, size)`
/// (or `.failed(message)` on error). Cancellation reverts to `.notDownloaded`.
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

    /// Background URLSession identifier — used by both the live session and a
    /// future `application(_:handleEventsForBackgroundURLSession:…)` hook.
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
    /// Inverse — used by `cancel(episodeID:)` to find the live task.
    var episodeIDToTask: [UUID: URLSessionDownloadTask] = [:]
    /// Last published progress value per episode — drives the 5% throttle.
    var lastPublishedProgress: [UUID: Double] = [:]
    /// Wall-clock of the last progress publish — drives the 200 ms throttle.
    var lastPublishedAt: [UUID: Date] = [:]

    /// The store the service mutates. Wired from `RootView.onAppear` so the
    /// service stays a singleton without owning a strong reference at init time.
    weak var appStore: AppStateStore?

    // MARK: Network monitoring (Wi-Fi guard for AutoDownloadPolicy)

    private let pathMonitor = NWPathMonitor()
    private let pathQueue = DispatchQueue(label: "io.f7z.podcast.downloads.path")
    /// Wraps the cached Wi-Fi flag so we can mutate from a background queue
    /// (NWPathMonitor) without tangling with `@Observable`'s tracking.
    private let pathState = PathState()

    /// Snapshot of the most recent network path — `true` when Wi-Fi is the
    /// preferred interface. Used by `evaluateAutoDownload` to honour
    /// `AutoDownloadPolicy.wifiOnly`.
    var isOnWiFi: Bool { pathState.isWiFi }

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
            pathState.set(isWiFi)
            if isWiFi {
                Task { @MainActor in
                    EpisodeDownloadService.shared.resumeQueuedDownloadsIfPossible()
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

    /// Starts (or resumes) a download for the episode with `episodeID`.
    /// No-op when the episode is already downloading or downloaded.
    func download(episodeID: UUID) {
        guard let store = appStore,
              let episode = store.episode(id: episodeID) else {
            logger.error("download(\(episodeID, privacy: .public)) — store/episode missing")
            return
        }
        switch episode.downloadState {
        case .downloading, .downloaded:
            return
        default:
            break
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
        task.taskDescription = episodeID.uuidString
        episodeIDToTask[episodeID] = task
        taskIDToEpisodeID[task.taskIdentifier] = episodeID
        progress[episodeID] = 0
        expectedBytes[episodeID] = nil
        lastPublishedProgress[episodeID] = 0
        lastPublishedAt[episodeID] = Date()

        store.setEpisodeDownloadState(episodeID, state: .downloading(progress: 0, bytesWritten: nil))
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

    /// Cancels the in-flight download for `episodeID`. Persists resume data
    /// where the server supports it so a later `download(episodeID:)` can pick
    /// up from the byte we left off at.
    func cancel(episodeID: UUID) {
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
        taskIDToEpisodeID = taskIDToEpisodeID.filter { $0.value != episodeID }
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

// MARK: - PathState

/// Tiny lock-guarded box for the cached Wi-Fi flag. Kept outside `@Observable`
/// because the `NWPathMonitor` callback runs on a background queue and must
/// not touch main-actor state directly.
final class PathState: @unchecked Sendable {
    private let lock = NSLock()
    private var _isWiFi: Bool = false

    var isWiFi: Bool {
        lock.lock(); defer { lock.unlock() }
        return _isWiFi
    }

    func set(_ value: Bool) {
        lock.lock(); defer { lock.unlock() }
        _isWiFi = value
    }
}
