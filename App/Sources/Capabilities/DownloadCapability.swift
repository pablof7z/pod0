import Foundation
import os.log

// MARK: - Download capability — `nmp.download.capability`
//
// iOS half of the download capability defined in
// `apps/nmp-app-podcast/src/capability/download.rs` (M4.A). The capability
// translates `DownloadCommand` JSON into background `URLSession` operations
// and pushes `DownloadReport` JSON back to Rust through an asynchronous
// `sendReport` channel.
//
// Doctrine:
//   D6 — errors never throw across the boundary. Bad URLs, transport
//        failures, missing tasks all surface as `DownloadReport.failed`
//        or a populated error envelope.
//   D7 — this capability *executes and reports*; it never decides queue
//        order, concurrency, retry policy, or auto-download policy. Every
//        such decision lives in the Rust `DownloadQueue` /
//        `podcast-feeds::refresh::policy` (M4.B Rust scope).
//   D8 — `Progress` reports are throttled to ≥1% bytes-delta AND ≥1 s
//        since the previous emit (AND-gate so both the brief and the
//        Rust schema's ≤1 Hz budget hold on slow connections).
//
// File-length budget: this file is the dispatch / lifecycle / command
// translation core. The `URLSessionDownloadDelegate` adapter lives in
// `DownloadCapability+Delegate.swift`; on-disk helpers (resume blobs,
// destination directory) live in `DownloadCapability+Storage.swift`. The
// wire vocabulary (`DownloadCommand`, `DownloadReport`) lives in
// `DownloadCapability+Wire.swift`.
//
// Background session reconnection (M4.C manual gate "kill app mid-download;
// relaunch; download resumes"):
//   The session identifier (`io.f7z.podcast.downloads`) is stable across
//   launches. On `init`, `URLSession.getAllTasks` rehydrates the
//   `episode_id → task` map from each task's `taskDescription` (set on
//   every `StartDownload`). `UIApplicationDelegate
//   .application(_:handleEventsForBackgroundURLSession:completionHandler:)`
//   in `PodcastApp` forwards the OS handoff into
//   `handleEventsForBackgroundURLSession(identifier:completionHandler:)`;
//   the delegate's `urlSessionDidFinishEvents(forBackgroundURLSession:)`
//   invokes it after the accrued events are drained.

/// Background-`URLSession`-backed executor for the download capability.
///
/// Single-instance, owned by `PodcastCapabilities`. State is the live
/// `URLSession` + the `episode_id → URLSessionDownloadTask` map; every
/// decision (which item to start next, whether to retry on failure)
/// lives in Rust.
@MainActor
final class DownloadCapability {
    nonisolated static let namespace = "nmp.download.capability"

    /// Background `URLSession` identifier. Matches the legacy app's
    /// identifier on purpose: a stable identifier lets the OS reattach
    /// pending downloads from the previous launch (or from the legacy
    /// Swift app on the first NMP launch) without losing progress.
    nonisolated static let sessionIdentifier = "io.f7z.podcast.downloads"

    private let logger = Logger(subsystem: "io.f7z.podcast", category: "DownloadCapability")

    // ── URLSession state owned by this executor ──────────────────────────
    private let coordinator: NmpDownloadCoordinator
    let session: URLSession

    // ── Map state — touched both here and from the delegate (which hops
    //   back to the main actor before mutating). The delegate reads the
    //   maps to recover `episode_id` after a background relaunch.
    var taskByEpisode: [String: URLSessionDownloadTask] = [:]
    /// Throttle bookkeeping per episode: last emitted bytes and last emit
    /// wall-clock. D8 — emit a `Progress` only when both gates are open.
    var lastEmittedBytes: [String: UInt64] = [:]
    var lastEmittedAt: [String: Date] = [:]

    // ── Out-of-band event sink to Rust ──────────────────────────────────
    /// Defaults to a no-op so the executor is exercisable from tests /
    /// previews; the kernel wires the real bridge via `attach(sendReport:)`
    /// once the canonical FFI surface for async iOS → Rust events lands
    /// (mirrors `AudioCapability` from M3.B).
    private var sendReport: (String) -> Void = { _ in }

    // ── Background-session OS handoff ───────────────────────────────────
    /// Completion handler the OS hands us via
    /// `application(_:handleEventsForBackgroundURLSession:completionHandler:)`.
    /// Held until `urlSessionDidFinishEvents` fires.
    private var backgroundCompletionHandlers: [String: () -> Void] = [:]

    private var started: Bool = false

    // MARK: Lifecycle

    init() {
        let coordinator = NmpDownloadCoordinator()
        self.coordinator = coordinator
        let config = URLSessionConfiguration.background(withIdentifier: Self.sessionIdentifier)
        config.isDiscretionary = false
        config.sessionSendsLaunchEvents = true
        config.allowsCellularAccess = true
        config.waitsForConnectivity = true
        self.session = URLSession(configuration: config, delegate: coordinator, delegateQueue: nil)
        coordinator.bind(capability: self)
        // Rehydrate any in-flight tasks from a prior launch. `getAllTasks`
        // is async; we don't block startup — the delegate callbacks that
        // fire before rehydration completes recover the episode id from
        // `taskDescription` regardless.
        rehydrateExistingTasks()
    }

    /// Idempotent. Marks the executor active and installs the report
    /// channel. Safe to call on every app foreground.
    func attach(sendReport: @escaping (String) -> Void) {
        self.sendReport = sendReport
        start()
    }

    /// Idempotent. Marks the executor active without installing a report
    /// channel — used by `PodcastCapabilities.start()`.
    func start() {
        guard !started else { return }
        started = true
    }

    /// Idempotent. Marks the executor inactive. Does NOT cancel in-flight
    /// downloads — the OS keeps the background session draining and
    /// late delegate callbacks still land in the report sink.
    func stop() {
        started = false
    }

    var isStarted: Bool { started }

    // MARK: - Background-session OS handoff

    /// Wired from `PodcastAppDelegate` (see
    /// `PodcastApp+BackgroundDownloads.swift`). Stores the handler until
    /// the delegate signals all accrued events are drained.
    func handleEventsForBackgroundURLSession(
        identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        guard identifier == Self.sessionIdentifier else {
            completionHandler()
            return
        }
        backgroundCompletionHandlers[identifier] = completionHandler
    }

    /// Called from the delegate's `urlSessionDidFinishEvents`. Pops the
    /// stored OS completion handler (if any) and invokes it on the main
    /// thread, signalling "all background events processed".
    func finishBackgroundEvents(for session: URLSession) {
        let identifier = session.configuration.identifier ?? Self.sessionIdentifier
        guard let handler = backgroundCompletionHandlers.removeValue(forKey: identifier) else {
            return
        }
        handler()
    }

    // MARK: - Command entry points

    /// Decode a `CapabilityRequest` JSON envelope and execute the
    /// contained `DownloadCommand`. Honors D6: malformed input degrades
    /// to an error envelope, never throws.
    @discardableResult
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            logger.debug("DownloadCapability: malformed request envelope")
            return errorEnvelope(correlationID: "", message: "malformed-request")
        }
        guard
            let payload = request.payloadJSON.data(using: .utf8),
            let command = try? JSONDecoder().decode(DownloadCommand.self, from: payload)
        else {
            logger.debug("DownloadCapability: malformed command payload for request \(request.correlationID)")
            return errorEnvelope(correlationID: request.correlationID, message: "malformed-payload")
        }
        logger.debug("DownloadCapability: decoded command: \(String(describing: command))")
        execute(command)
        return okEnvelope(correlationID: request.correlationID)
    }

    /// Direct command entry — used by tests and any future synchronous
    /// caller. The capability does not "decide" anything; `execute(_:)`
    /// is a pure `URLSession` translation of the command.
    func execute(_ command: DownloadCommand) {
        switch command {
        case let .startDownload(url, episodeID, kind, _):
            logger.debug("DownloadCapability: executing startDownload episodeID=\(episodeID) kind=\(kind.rawValue) url=\(url)")
            startDownload(url: url, episodeID: episodeID, kind: kind)
        case let .pauseDownload(episodeID):
            logger.debug("DownloadCapability: executing pauseDownload episodeID=\(episodeID)")
            pauseDownload(episodeID: episodeID)
        case let .resumeDownload(episodeID):
            logger.debug("DownloadCapability: executing resumeDownload episodeID=\(episodeID)")
            resumeDownload(episodeID: episodeID)
        case let .cancelDownload(episodeID):
            logger.debug("DownloadCapability: executing cancelDownload episodeID=\(episodeID)")
            cancelDownload(episodeID: episodeID)
        case .cancelAll:
            logger.debug("DownloadCapability: executing cancelAll")
            cancelAll()
        }
    }

    // MARK: - Command implementations

    private func startDownload(url: String, episodeID: String, kind: DownloadKind = .episode) {
        // Already in flight? D7 — we don't decide policy. If Rust asks
        // again, ignore the duplicate (the existing task will emit its
        // own reports); we never preempt a live task with a fresh one
        // because that would lose bytes.
        if taskByEpisode[episodeID] != nil {
            logger.debug("DownloadCapability: startDownload already in flight for \(episodeID)")
            return
        }
        guard let url = URL(string: url) else {
            logger.error("DownloadCapability: invalid URL for \(episodeID): \(url)")
            emit(.failed(episodeID: episodeID, error: "invalid-url"))
            return
        }
        let task: URLSessionDownloadTask
        if let resumeData = Self.loadResumeData(for: episodeID, kind: kind) {
            logger.debug("DownloadCapability: resuming download for \(episodeID)")
            task = session.downloadTask(withResumeData: resumeData)
        } else {
            logger.debug("DownloadCapability: starting new download for \(episodeID) from \(url.absoluteString)")
            task = session.downloadTask(with: url)
        }
        // Encode the kind into taskDescription so the delegate can recover where
        // to write on a background relaunch (when taskByEpisode is empty).
        // Episodes keep the bare id — back-compat with tasks from earlier builds.
        task.taskDescription = Self.encodeTaskDescription(episodeID: episodeID, kind: kind)
        taskByEpisode[episodeID] = task
        lastEmittedBytes[episodeID] = 0
        lastEmittedAt[episodeID] = .distantPast
        task.resume()
        logger.debug("DownloadCapability: resumed URLSessionDownloadTask for \(episodeID)")
    }

    // MARK: - taskDescription kind encoding

    /// `taskDescription` carries the kind for non-episodes (`local_model:<id>`)
    /// so a background relaunch — where only the OS-persisted task survives —
    /// can recover the destination. Episodes keep the bare id unchanged.
    nonisolated static func encodeTaskDescription(episodeID: String, kind: DownloadKind) -> String {
        kind == .episode ? episodeID : "\(kind.rawValue):\(episodeID)"
    }

    /// Inverse of `encodeTaskDescription`. Unknown / unprefixed descriptions
    /// decode as `.episode` (the historical format).
    nonisolated static func decodeTaskDescription(_ description: String) -> (episodeID: String, kind: DownloadKind) {
        if let colon = description.firstIndex(of: ":"),
           let kind = DownloadKind(rawValue: String(description[..<colon])) {
            return (String(description[description.index(after: colon)...]), kind)
        }
        return (description, .episode)
    }

    private func pauseDownload(episodeID: String) {
        guard let task = taskByEpisode[episodeID] else { return }
        let kind = Self.decodeTaskDescription(task.taskDescription ?? episodeID).kind
        // `cancel(byProducingResumeData:)` completes asynchronously on
        // an arbitrary queue; hop back so map mutation stays
        // main-actor-isolated.
        task.cancel { [weak self] resumeData in
            // Read `countOfBytesReceived` *inside* the cancel closure
            // (i.e. on the URLSession delegate queue) — it is the
            // authoritative byte counter the OS maintained as bytes
            // arrived. `lastEmittedBytes` only ticks past the D8
            // throttle gate; if the user pauses before the first emit
            // (small file, fast pause, sub-1% sub-1 s progress) it is
            // still `0` while the task has pulled real bytes. The
            // resume token recorded by URLSession captures the real
            // offset regardless; this just keeps the projection
            // honest.
            let bytesSoFar = UInt64(max(0, task.countOfBytesReceived))
            Task { @MainActor [weak self] in
                guard let self else { return }
                if let resumeData {
                    Self.writeResumeData(resumeData, for: episodeID, kind: kind)
                }
                self.taskByEpisode[episodeID] = nil
                self.emit(.paused(episodeID: episodeID, bytesDownloaded: bytesSoFar))
            }
        }
    }

    private func resumeDownload(episodeID: String) {
        // Resume requires a saved blob (the original URL is gone from the
        // executor by the time `Pause` lands). D7 — if we don't have one,
        // surface the fact; Rust decides whether to re-issue a
        // `StartDownload` with the URL.
        if taskByEpisode[episodeID] != nil { return }
        // The ResumeDownload command carries only the id (the task is gone), so
        // recover the kind from which resume blob exists on disk — model blobs
        // are kind-prefixed (`local_model_<id>.data`). This keeps both the
        // resume read AND the re-stamped taskDescription consistent with the
        // original download, so a background-relaunch completion writes the file
        // to the right directory and runs the right persistence path.
        let kind: DownloadKind = Self.loadResumeData(for: episodeID, kind: .localModel) != nil
            ? .localModel : .episode
        guard let resumeData = Self.loadResumeData(for: episodeID, kind: kind) else {
            emit(.failed(
                episodeID: episodeID,
                error: "no-resume-data: re-issue start_download"))
            return
        }
        let task = session.downloadTask(withResumeData: resumeData)
        task.taskDescription = Self.encodeTaskDescription(episodeID: episodeID, kind: kind)
        taskByEpisode[episodeID] = task
        lastEmittedBytes[episodeID] = 0
        lastEmittedAt[episodeID] = .distantPast
        task.resume()
    }

    private func cancelDownload(episodeID: String) {
        guard let task = taskByEpisode[episodeID] else {
            // Idempotent — a cancel for an unknown id is data, not error.
            // We still surface `Cancelled` so the kernel projection
            // reaches a consistent state.
            emit(.cancelled(episodeID: episodeID))
            return
        }
        let kind = Self.decodeTaskDescription(task.taskDescription ?? episodeID).kind
        task.cancel()
        taskByEpisode[episodeID] = nil
        Self.clearResumeData(for: episodeID, kind: kind)
        emit(.cancelled(episodeID: episodeID))
    }

    private func cancelAll() {
        let active = Array(taskByEpisode.keys)
        for episodeID in active {
            cancelDownload(episodeID: episodeID)
        }
    }

    // MARK: - Reporting helpers (also called from `+Delegate.swift`)

    func emit(_ report: DownloadReport) {
        guard let json = report.jsonString() else {
            logger.error("download report encode failed: \(String(describing: report), privacy: .public)")
            return
        }
        sendReport(json)
        DiagnosticLog.shared.append(
            level: .info, category: "download",
            message: "report: \(String(describing: report))")
    }

    /// D8 progress throttle. Emits a `Progress` only when both gates open:
    ///   - **≥1% bytes-delta** when `totalBytes` is known (the brief's
    ///     "throttle to ≥1% change"), OR ≥256 KiB absolute delta when
    ///     `totalBytes` is `nil` (stand-in for "meaningful change" before
    ///     the server reports `Content-Length`), AND
    ///   - **≥1 s** since the previous emit (the Rust schema's ≤1 Hz
    ///     budget).
    func emitProgressIfThresholdCrossed(
        episodeID: String,
        bytesDownloaded: UInt64,
        totalBytes: UInt64?
    ) {
        let last = lastEmittedBytes[episodeID] ?? 0
        let lastAt = lastEmittedAt[episodeID] ?? .distantPast
        let now = Date()
        let bytesDelta = bytesDownloaded > last ? bytesDownloaded - last : 0
        let percentGate: Bool = {
            if let totalBytes, totalBytes > 0 {
                return Double(bytesDelta) / Double(totalBytes) >= 0.01
            }
            return bytesDelta >= 256 * 1024
        }()
        let timeGate = now.timeIntervalSince(lastAt) >= 1.0
        guard percentGate && timeGate else { return }
        lastEmittedBytes[episodeID] = bytesDownloaded
        lastEmittedAt[episodeID] = now
        emit(.progress(
            episodeID: episodeID,
            bytesDownloaded: bytesDownloaded,
            totalBytes: totalBytes))
    }

    // Background-relaunch task rehydration + envelope encoding live in
    // `DownloadCapability+Envelope.swift` (AGENTS.md 300-LOC soft limit).
}
