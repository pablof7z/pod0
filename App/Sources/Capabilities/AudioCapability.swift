import AVFoundation
import Foundation
import MediaPlayer
import os.log

// MARK: - Audio capability — `nmp.audio.capability`
//
// iOS half of the audio capability defined in
// `apps/nmp-app-podcast/src/capability/audio.rs`. Rust dispatches an
// `AudioCommand`; this executor runs it against `AVPlayer` and pushes
// `AudioReport` values back to Rust through the `sendReport` closure.
//
// Doctrine:
//   D6 — errors never throw across the boundary. A bad URL, a failed
//        seek, an AVPlayer error all surface as `AudioReport.failed`.
//   D7 — this capability *executes and reports*; it never decides what
//        plays next, when to stop on sleep-timer expiry, or how to react
//        to an `Ended`. Every such decision lives in
//        `crate::player::PlayerActor`. Remote-command-center taps round-
//        trip back to Rust as reports so the kernel runs the policy.
//   D8 — `Playing` position reports are throttled to 1 Hz (`AudioReport`
//        schema doc says ≤4 Hz; the M3.B brief tightens to ≤1 Hz which
//        satisfies both). The kernel collapses bursts into the next tick.
//   D9 — Sleep-timer expiry is a Rust decision. The iOS-side timer fires
//        a `SleepTimerFired` report; the actor replies with a `Stop`
//        command we then execute. We never call `pause()` or `stop()`
//        ourselves on timer expiry.
//
// File-length budget: this file is the dispatch / state / time-observer
// core; AVAudioSession setup, MPNowPlayingInfoCenter, and
// MPRemoteCommandCenter wiring are in sibling files
// (`AudioCapability+Session.swift`, `+NowPlaying.swift`,
// `+RemoteCommands.swift`) to honor the 300-line soft limit (AGENTS.md).
//
// FFI seam: the `sendReport` closure is the asynchronous iOS → Rust
// channel for events the kernel didn't synchronously request (time-ticks,
// remote-command taps, item-end). The current FFI surface has no async
// event channel; M3.B's kernel-side `ActionModule`/`CapabilityModule`
// wiring will route reports through
// `crate::capability::dispatch::dispatch_audio_report_json`. Until then
// the closure defaults to a no-op so the build is green and the
// executor's lifecycle is exercisable from unit tests.

// The wire vocabulary (`AudioCommand`, `AudioReport`) lives in
// `AudioCapability+Wire.swift` so this file stays focused on the
// executor itself.

// MARK: - Executor

/// `AVPlayer`-backed executor for the audio capability.
///
/// Single-instance, owned by `PodcastCapabilities`. State is the live
/// `AVPlayer` + its current item + the periodic time observer token;
/// every decision (next episode, sleep-timer policy, end-of-queue) lives
/// in Rust.
@MainActor
final class AudioCapability: NSObject {
    static let namespace = "nmp.audio.capability"

    private let logger = Logger(subsystem: "io.f7z.podcast", category: "AudioCapability")

    // ── Player state owned by this executor ──────────────────────────────
    private let player: AVPlayer = AVPlayer()
    private var currentURL: String?
    private var timeObserverToken: Any?
    private var endObserver: NSObjectProtocol?
    private var statusObservation: NSKeyValueObservation?
    private var loadedRangesObservation: NSKeyValueObservation?
    private var lastBufferedFraction: Float = -1

    // ── Sleep-timer (system-level, fires `SleepTimerFired` report) ──────
    private var sleepTimer: DispatchSourceTimer?

    // ── Out-of-band event sink to Rust ──────────────────────────────────
    /// The asynchronous report channel. Defaults to a no-op so the
    /// executor is exercisable from unit tests and previews; the kernel
    /// wires the real bridge via `attach(sendReport:)` once the
    /// `dispatch_audio_report_json` plumbing lands (M3.B kernel side).
    private var sendReport: (String) -> Void = { _ in }

    // ── Engine bridge (M1 Part 3) ────────────────────────────────────────
    /// When set, `execute(_:)` forwards every command here instead of
    /// acting on the own `AVPlayer`. Used to bridge Rust-originated
    /// `AudioCommand`s to `AudioEngine` while the two-player architecture
    /// is in place. `nil` = standalone mode (own `AVPlayer`, unit tests).
    var commandHandler: ((AudioCommand) -> Void)?

    private var started: Bool = false
    private var hasConfiguredAudioSession: Bool = false

    // MARK: Lifecycle

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
        installRemoteCommands()
    }

    /// Idempotent. Marks the executor inactive. Does NOT release
    /// AVPlayer or audio session — late `Stop`/`Pause` commands still
    /// land.
    func stop() {
        started = false
        cancelSleepTimer()
        removeRemoteCommands()
    }

    var isStarted: Bool { started }

    // MARK: Command entry point

    /// Decode a `CapabilityRequest` JSON envelope and execute the
    /// contained `AudioCommand`. Honors D6: malformed input degrades to
    /// an error envelope, never throws.
    @discardableResult
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            return errorEnvelope(correlationID: "", message: "malformed-request")
        }
        guard
            let payload = request.payloadJSON.data(using: .utf8),
            let command = try? JSONDecoder().decode(AudioCommand.self, from: payload)
        else {
            return errorEnvelope(correlationID: request.correlationID, message: "malformed-payload")
        }
        execute(command)
        return okEnvelope(correlationID: request.correlationID)
    }

    /// Direct command entry — used by remote-command handlers and the
    /// dispatch tests. The capability does not "decide" anything;
    /// `execute(_:)` is a pure AVFoundation translation of the command.
    ///
    /// When `commandHandler` is installed (M1 bridge mode), ALL commands
    /// except `setSleepTimer` are forwarded there and this method returns
    /// early — the own `AVPlayer` is bypassed. Sleep-timer duration mode is
    /// still held by the capability's OS timer so expiry reports flow back to
    /// Rust; Rust remains the policy owner.
    func execute(_ command: AudioCommand) {
        if case let .setSleepTimer(secs) = command {
            armSleepTimer(secs: secs)
            return
        }
        if let handler = commandHandler {
            handler(command)
            return
        }
        switch command {
        case let .load(url, positionSecs, _):
            loadAndSeek(url: url, positionSecs: positionSecs)
        case .play:
            playerPlay()
        case .pause:
            playerPause()
        case let .seek(positionSecs):
            playerSeek(to: positionSecs)
        case let .setVolume(volume):
            player.volume = clamp(volume, min: 0, max: 1)
        case let .setSpeed(speed):
            let rate = clamp(speed, min: 0.5, max: 3.0)
            if player.timeControlStatus == .playing {
                player.rate = rate
            }
        case .setSleepTimer:
            break
        case .stop:
            playerStop()
        }
    }

    // MARK: - Command implementations

    private func loadAndSeek(url: String, positionSecs: Double) {
        guard let assetURL = URL(string: url) else {
            emit(.failed(url: url, error: "invalid-url"))
            return
        }
        currentURL = url
        lastBufferedFraction = -1

        // Best-effort audio-session prime so the first play() has speakers.
        configureAudioSessionIfNeeded()

        teardownItemObservers()
        let asset = AVURLAsset(url: assetURL)
        let item = AVPlayerItem(asset: asset)
        player.replaceCurrentItem(with: item)
        installItemObservers(for: item)
        installTimeObserverIfNeeded()

        if positionSecs > 0 {
            let target = CMTime(seconds: positionSecs, preferredTimescale: 600)
            player.seek(to: target, toleranceBefore: .zero, toleranceAfter: .zero)
        }
    }

    private func playerPlay() {
        configureAudioSessionIfNeeded()
        player.play()
        // The time-observer / KVO will emit the first `Playing` report
        // once AVFoundation reports `timeControlStatus == .playing` and
        // the periodic tick fires.
    }

    private func playerPause() {
        player.pause()
        emit(.paused(
            url: currentURL ?? "",
            positionSecs: currentPositionSecs()))
        updateNowPlayingPaused()
    }

    private func playerSeek(to positionSecs: Double) {
        let target = CMTime(seconds: max(0, positionSecs), preferredTimescale: 600)
        player.seek(to: target, toleranceBefore: .zero, toleranceAfter: .zero) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                // Emit a Playing or Paused report depending on the
                // active state so the kernel sees the new position
                // immediately without waiting for the next tick.
                self.emitCurrentStateReport()
            }
        }
    }

    private func playerStop() {
        cancelSleepTimer()
        player.pause()
        player.replaceCurrentItem(with: nil)
        teardownItemObservers()
        currentURL = nil
        // emit(.stopped) folds into `updateNowPlayingForReport(.stopped)`
        // which clears the lock-screen dictionary; no separate
        // clearNowPlaying() call needed.
        emit(.stopped)
    }

    // Observers live in `AudioCapability+Observers.swift` and reach into
    // the storage accessors at the bottom of this file.

    // MARK: - Sleep-timer (D9: iOS holds the wall-clock; Rust decides)

    private func armSleepTimer(secs: UInt64?) {
        cancelSleepTimer()
        guard let secs, secs > 0 else { return }
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now() + .seconds(Int(secs)))
        timer.setEventHandler { [weak self] in
            MainActor.assumeIsolated {
                self?.onSleepTimerFire()
            }
        }
        sleepTimer = timer
        timer.resume()
    }

    private func cancelSleepTimer() {
        sleepTimer?.cancel()
        sleepTimer = nil
    }

    private func onSleepTimerFire() {
        cancelSleepTimer()
        // D9: do NOT pause/stop here. The actor decides; we just report.
        emit(.sleepTimerFired)
    }

    // MARK: - Helpers

    private func emit(_ report: AudioReport) {
        guard let json = report.jsonString() else {
            logger.error("audio report encode failed: \(String(describing: report), privacy: .public)")
            return
        }
        sendReport(json)
        // Most reports also drive the Now Playing surface; fold the
        // playing/stopped/paused branches into the NowPlaying extension
        // so the wiring stays local.
        updateNowPlayingForReport(report)
    }

    private func emitCurrentStateReport() {
        if player.timeControlStatus == .playing {
            emit(.playing(
                url: currentURL ?? "",
                positionSecs: currentPositionSecs(),
                durationSecs: currentDurationSecs()))
        } else {
            emit(.paused(
                url: currentURL ?? "",
                positionSecs: currentPositionSecs()))
        }
    }

    private func currentPositionSecs() -> Double {
        let seconds = player.currentTime().seconds
        return seconds.isFinite ? max(0, seconds) : 0
    }

    private func currentDurationSecs() -> Double {
        guard let duration = player.currentItem?.duration.seconds,
              duration.isFinite, duration > 0
        else { return 0 }
        return duration
    }

    private func clamp<T: Comparable>(_ value: T, min lo: T, max hi: T) -> T {
        Swift.min(Swift.max(value, lo), hi)
    }

    // MARK: - Envelope encoding

    private func okEnvelope(correlationID: String) -> String {
        let env = CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: correlationID,
            resultJSON: "{\"status\":\"ok\"}")
        return Self.encode(env) ?? "{}"
    }

    private func errorEnvelope(correlationID: String, message: String) -> String {
        let payload = "{\"status\":\"error\",\"message\":\"\(message)\"}"
        let env = CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: correlationID,
            resultJSON: payload)
        return Self.encode(env) ?? "{}"
    }

    private static func encode<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    // ── Storage accessors for the sibling-file extensions ───────────────
    //
    // Swift extensions can't add stored properties, so the executor's
    // observer / now-playing / remote-command extensions reach into the
    // private state through narrow `internal` accessors. The accessors
    // are deliberately verbose (named getters/setters rather than
    // public-by-default `var`s) so the extension files can't accidentally
    // grow direct AVPlayer ownership.

    /// Whether `configureAudioSession()` has run successfully yet.
    var sessionConfigured: Bool {
        get { hasConfiguredAudioSession }
        set { hasConfiguredAudioSession = newValue }
    }

    /// Synchronous AVPlayer accessor — extensions read `timeControlStatus`
    /// and `rate` but never mutate the player directly.
    var avPlayer: AVPlayer { player }

    /// Expose the report sink to the RemoteCommands extension so taps
    /// from the lock screen / AirPods round-trip through Rust.
    func sendReportJSON(_ json: String) { sendReport(json) }

    /// Expose the report-emit helper to extensions for synthesised
    /// reports (e.g. a remote-command-driven seek confirmation).
    func emitReport(_ report: AudioReport) { emit(report) }

    /// Expose current url/position for the NowPlaying / Observers
    /// extensions.
    var currentTrackURL: String? { currentURL }
    func currentPosition() -> Double { currentPositionSecs() }
    func currentDuration() -> Double { currentDurationSecs() }

    // MARK: Observer state — owned here, mutated from `+Observers.swift`.

    var hasTimeObserver: Bool { timeObserverToken != nil }
    func setTimeObserverToken(_ token: Any) {
        timeObserverToken = token
    }

    func setItemObservers(
        status: NSKeyValueObservation?,
        loadedRanges: NSKeyValueObservation?,
        end: NSObjectProtocol?
    ) {
        statusObservation = status
        loadedRangesObservation = loadedRanges
        endObserver = end
    }

    /// Returns and clears the three observer slots in a single step so
    /// the extension's teardown can invalidate them outside the lock.
    func clearItemObservers() -> (NSKeyValueObservation?, NSKeyValueObservation?, NSObjectProtocol?) {
        let s = statusObservation
        let l = loadedRangesObservation
        let e = endObserver
        statusObservation = nil
        loadedRangesObservation = nil
        endObserver = nil
        return (s, l, e)
    }

    var lastBufferedFractionStorage: Float { lastBufferedFraction }
    func setLastBufferedFraction(_ value: Float) {
        lastBufferedFraction = value
    }
}
