import Foundation
import Observation
import SwiftUI
import os.log

private let kmLog = Logger(subsystem: "io.f7z.podcast", category: "KernelModel")

/// `@Observable` mirror of the kernel snapshot. The Rust actor pushes
/// JSON updates via the C callback; this class decodes them and republishes
/// for SwiftUI consumption.
///
/// Thin-shell: all state lives in `snapshot`. No business logic. No derived
/// caches. Every accessor is a pure read of the kernel snapshot (D2, D4, D8).
///
/// `@Observable` (not `ObservableObject`) so the migrated Identity / Agent /
/// Onboarding views can use `@Environment(KernelModel.self)`. Observation is
/// emitted automatically for plain stored properties.
@MainActor
@Observable
final class KernelModel {

    // ── External-scene shared handle ───────────────────────────────────────

    /// Process-wide weak handle to the live `KernelModel`. Used by surfaces
    /// that execute outside the SwiftUI environment chain — today that means
    /// iOS `AppIntent` performers (Siri/Shortcuts) and `CarPlaySceneDelegate`
    /// (a `UIResponder` instantiated by the OS for the
    /// `CPTemplateApplicationScene` role). Gives intent and scene code a
    /// stable pointer to the same model the UI is reading without forcing a
    /// singleton lifetime.
    ///
    /// `weak` so the model still deallocates on scene teardown; the next
    /// `KernelModel.init` re-publishes from its own initializer.
    ///
    /// `nonisolated(unsafe)` is required because `@MainActor`-isolated stored
    /// properties cannot be `static`. The lone writer is `KernelModel.init`
    /// (which runs on `@MainActor` per the class isolation); the readers are
    /// also `@MainActor`. The pointer is never published off the main thread.
    nonisolated(unsafe) static weak var shared: KernelModel?

    // ── Snapshot slot ──────────────────────────────────────────────────────

    /// Latest decoded snapshot. `nil` before the first tick lands.
    private(set) var snapshot: PodcastUpdate?

    // ── Local counters ─────────────────────────────────────────────────────

    private(set) var snapshotCount: UInt64 = 0
    private(set) var lastSnapshotAt: Date?

    /// Clearable toast text sourced from snapshot or synchronous dispatch
    /// rejections.
    private(set) var lastErrorToast: String?

    /// Identity projection slice (`active_account` / `accounts` /
    /// `bunker_handshake`) pulled out of the NMP-core kernel snapshot on
    /// every tick. Read-only — the kernel actor is the sole writer.
    /// `UserIdentityStore` mirrors its observable state from this field.
    private(set) var kernelIdentity: KernelIdentityProjection = .empty

    /// D7 actor-death surface — flips to `true` exactly once when the Rust
    /// supervisor emits a panic frame or the foreground-resume probe detects
    /// the actor gone. Terminal: only a process restart recovers.
    private(set) var kernelIsDead: Bool = false

    var visibleLimit: UInt32 = 80
    var emitHz: UInt32 = 4

    // ── Podcast projection (polled separately from NMP kernel snapshot) ───

    /// Latest podcast library decoded from `nmp_app_podcast_snapshot`.
    private(set) var library: [PodcastSummary] = []
    /// Latest full podcast snapshot (library, player, account …).
    private(set) var podcastSnapshot: PodcastUpdate?
    /// Cancellable for the 500ms poll Task.
    private var snapshotPollTask: Task<Void, Never>?

    // ── Computed projections ───────────────────────────────────────────────

    var isRunning: Bool { snapshot?.running ?? false }
    var rev: Int { snapshot?.rev ?? 0 }
    /// Non-optional convenience for the podcast settings projection.
    /// Falls back to `SettingsSnapshot()` (default values) before the
    /// first podcast snapshot tick — all callers get a coherent value.
    var settings: SettingsSnapshot { podcastSnapshot?.settings ?? SettingsSnapshot() }

    // ── Implementation ─────────────────────────────────────────────────────

    private let kernel = PodcastHandle()
    private var startedKernel = false

    init() {
        kernel.listen({ [weak self] result in
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                MainActor.assumeIsolated { self.apply(result: result) }
            }
        }, onPanic: { [weak self] in
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                MainActor.assumeIsolated { self.markKernelDead() }
            }
        })
        kernel.attachAudioReportChannel()
        kernel.attachDownloadReportChannel()
        // Publish to the shared handle for external scenes (CarPlay, AppIntents,
        // …). The static is `weak`, so the model still deallocates on scene
        // teardown; the next instance re-publishes from its own `init`.
        Self.shared = self
        kernel.attachVoiceReportChannel()
    }

    private func markKernelDead() {
        if kernelIsDead { return }
        kmLog.fault("kernelIsDead set — actor thread terminated")
        kernelIsDead = true
    }

    /// Pull-side actor-liveness probe (ADR-0028). Called by the app on every
    /// `.active` scenePhase to catch panics that occurred while backgrounded.
    func checkAlive() {
        if kernelIsDead { return }
        if !kernel.isAlive() {
            markKernelDead()
        }
    }

    // ── Lifecycle ──────────────────────────────────────────────────────────

    func start() {
        guard !startedKernel else { return }
        startedKernel = true
        kernel.start(visibleLimit: visibleLimit, emitHz: emitHz)
        startSnapshotPoll()
    }

    func stop() {
        snapshotPollTask?.cancel()
        snapshotPollTask = nil
        kernel.stop()
        startedKernel = false
        cleanupCompatState()
    }

    func resetAndRestart() {
        snapshotPollTask?.cancel()
        snapshotPollTask = nil
        kernel.reset()
        snapshot = nil
        podcastSnapshot = nil
        library = []
        kernel.reregisterPodcastProjection()
        lastErrorToast = nil
        kernel.start(visibleLimit: visibleLimit, emitHz: emitHz)
        startedKernel = true
        startSnapshotPoll()
    }

    private func startSnapshotPoll() {
        snapshotPollTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(500))
                guard !Task.isCancelled else { break }
                await MainActor.run { [weak self] in
                    self?.pullPodcastSnapshotIfChanged()
                }
            }
        }
    }

    /// Pull the podcast snapshot and apply it if the rev has advanced.
    /// Called both by the 500ms background poll (for autonomous changes like
    /// download progress and playback position) and immediately after every
    /// `dispatch` / `dispatchSilent` call so user actions are reflected in
    /// the UI within the same runloop pass rather than after up to 500ms.
    private func pullPodcastSnapshotIfChanged() {
        // Cheap rev-check before the full JSON decode. nmp_app_podcast_snapshot_rev
        // reads a single atomic u64 — no serialization cost.
        let currentRev = kernel.podcastSnapshotRev()
        guard currentRev > UInt64(podcastSnapshot?.rev ?? 0) else { return }
        let update = kernel.podcastSnapshot()
        guard update.rev > (podcastSnapshot?.rev ?? 0) else { return }
        let previousNowPlaying = podcastSnapshot?.nowPlaying
        podcastSnapshot = update
        library = update.library
        PodcastCapabilities.shared.iCloudSync.applySettingsSnapshot(
            SettingsKVSnapshot.from(podcastUpdate: update))
        PodcastCapabilities.shared.spotlight.indexLibrary(update.library)
        reconcileLiveActivity(
            previous: previousNowPlaying, next: update.nowPlaying, library: update.library)
        kmLog.debug("podcast snapshot updated rev=\(update.rev) library=\(update.library.count)")
    }

    /// Translate `PlayerState` transitions into Live Activity lifecycle
    /// calls. Driven exclusively by `startSnapshotPoll` — every kernel
    /// snapshot advance is the one place that can change the now-playing
    /// surface, so this is the single funnel that mirrors that state out
    /// to ActivityKit (D7 — kernel is the source of truth, executor only
    /// translates).
    ///
    /// Transitions handled:
    ///   - nil → non-nil: `start(...)` with episode metadata pulled from
    ///     the embedded library (titles + artwork live on `EpisodeSummary`
    ///     and `PodcastSummary`, not on `PlayerState`).
    ///   - non-nil → non-nil (same episode): `update(positionSecs:isPlaying:)`,
    ///     which the manager already throttles to ~1 Hz.
    ///   - non-nil → non-nil (different episode): the manager's `start`
    ///     handles the end → request roundtrip itself.
    ///   - non-nil → nil: `stop()`.
    private func reconcileLiveActivity(
        previous: PlayerState?, next: PlayerState?, library: [PodcastSummary]
    ) {
        switch (previous, next) {
        case (nil, nil):
            return
        case (_, nil):
            LiveActivityManager.shared.stop()
        case let (nil, .some(state)):
            startLiveActivity(for: state, library: library)
        case let (.some(prev), .some(state)):
            if prev.episodeId != state.episodeId {
                startLiveActivity(for: state, library: library)
            } else {
                LiveActivityManager.shared.update(
                    positionSecs: state.positionSecs, isPlaying: state.isPlaying)
            }
        }
    }

    /// Resolve episode/podcast metadata from the library snapshot and
    /// hand the manager a fully-populated start payload. The library is
    /// the only place titles/artwork live — `PlayerState` itself is
    /// metadata-poor by design (it carries only what the audio engine
    /// needs).
    private func startLiveActivity(for state: PlayerState, library: [PodcastSummary]) {
        guard let episodeId = state.episodeId else { return }
        var episodeTitle = ""
        var podcastTitle = ""
        var artworkURL: URL?

        outer: for show in library {
            for episode in show.episodes where episode.id == episodeId {
                episodeTitle = episode.title
                podcastTitle = episode.podcastTitle ?? show.title
                let artworkString = episode.artworkUrl ?? show.artworkUrl
                if let artworkString { artworkURL = URL(string: artworkString) }
                break outer
            }
        }
        if episodeTitle.isEmpty { episodeTitle = "Now Playing" }

        LiveActivityManager.shared.start(
            episodeID: episodeId,
            episodeTitle: episodeTitle,
            podcastTitle: podcastTitle,
            positionSecs: state.positionSecs,
            durationSecs: state.durationSecs ?? 0,
            isPlaying: state.isPlaying,
            artworkURL: artworkURL)
    }

    func applyConfiguration() {
        kernel.configure(visibleLimit: visibleLimit, emitHz: emitHz)
    }

    // ── scenePhase pass-through ────────────────────────────────────────────

    /// True until the first `.active` scenePhase has been observed. Cold
    /// start already drives a fresh snapshot through `start()`, so the
    /// initial activation must NOT pile a `refresh_all` on top of it —
    /// only subsequent foreground returns trigger a feed sync.
    private var hasObservedForeground = false

    func lifecycleForeground() {
        kernel.lifecycleForeground()
        guard hasObservedForeground else {
            hasObservedForeground = true
            return
        }
        dispatch(namespace: "podcast", body: ["op": "refresh_all"])
    }
    func lifecycleBackground() { kernel.lifecycleBackground() }

    // ── Toast ──────────────────────────────────────────────────────────────

    func clearErrorToast() { lastErrorToast = nil }

    /// Set the toast surface from outside this file. Used by features
    /// (notably `Features/Identity/IdentityViewModel.swift`) that need to
    /// route a staged-action notice through the same banner channel as
    /// synchronous dispatch failures.
    ///
    /// `private(set)` on `lastErrorToast` restricts writes to this file;
    /// callers in other files use this entry point instead of touching
    /// the property directly.
    func setErrorToast(_ message: String?) {
        lastErrorToast = message
    }

    // ── Dispatch ───────────────────────────────────────────────────────────

    /// Fire-and-forget generic dispatch. Surfaces synchronous rejections as a
    /// toast (D6 — outcomes always arrive in-band; never throws).
    @discardableResult
    func dispatch(namespace: String, body: [String: Any]) -> DispatchResult {
        let result = kernel.dispatchAction(namespace: namespace, body: body)
        if case let .failure(message) = result {
            kmLog.error("dispatch_action rejected: \(message, privacy: .public)")
            lastErrorToast = message
        }
        pullPodcastSnapshotIfChanged()
        return result
    }

    /// Identical to `dispatch(namespace:body:)` but logs failures instead of
    /// surfacing them as a user-visible toast. For callers that are
    /// best-effort and where a transient rejection (e.g. an action the
    /// kernel has not registered yet) is expected and should stay
    /// developer-only. Today the only caller is `iCloudSyncCapability`,
    /// which dispatches `podcast.settings.*` actions whose Rust handlers
    /// land in a follow-up PR.
    @discardableResult
    func dispatchSilent(namespace: String, body: [String: Any]) -> DispatchResult {
        let result = kernel.dispatchAction(namespace: namespace, body: body)
        if case let .failure(message) = result {
            kmLog.error("dispatch_action (silent) rejected: \(message, privacy: .public)")
        }
        pullPodcastSnapshotIfChanged()
        return result
    }

    // ── Identity / NIP-46 ────────────────────────────────────────────────
    //
    // Typed wrappers around the NMP-core identity FFI. `UserIdentityStore`
    // calls these instead of touching the raw `PodcastHandle`. The actor
    // confirms the resulting state on the next snapshot tick via
    // `KernelIdentityProjection` — no synchronous return.

    /// Begin a `bunker://` sign-in. Fire-and-forget — observe
    /// `identity.bunkerHandshake` / `identity.activeAccount` for outcome.
    /// Silent no-op (D6) if `nmp_signer_broker_init` was never called.
    func signInBunker(uri: String) {
        kernel.signInBunker(uri: uri)
    }

    /// Begin an nsec sign-in. The secret crosses the FFI boundary as raw
    /// bytes (it has to be imported somehow) and is wrapped in `Zeroizing`
    /// the instant the actor receives it (see
    /// `crates/nmp-ffi/src/identity.rs::nmp_app_signin_nsec`). The Rust
    /// `ActorCommand::SignInNsec` handler validates and persists the key
    /// via the kernel keyring path — DO NOT also write to
    /// `PcstIdentityCapability` here. Single source of truth.
    func signInNsec(_ nsec: String) {
        kernel.signInNsec(nsec)
    }

    /// Cancel the in-flight bunker handshake. Safe / idempotent when nothing
    /// is in flight.
    func cancelBunkerHandshake() {
        kernel.cancelBunkerHandshake()
    }

    /// Generate a fresh `nostrconnect://` URI for client-initiated NIP-46
    /// pairing. The broker is already listening for the signer app's
    /// response on the embedded relay — handing the URI to the user (QR or
    /// deep-link) is the only host responsibility. `callbackScheme` should
    /// be `nil` when the host URL scheme is not registered with the OS.
    func nostrconnectURI(relayURL: String? = nil, callbackScheme: String? = nil) -> String? {
        kernel.nostrconnectURI(relayURL: relayURL, callbackScheme: callbackScheme)
    }

    /// Remove the active account from the kernel. Mirrored on the next
    /// snapshot tick via `identity.activeAccount` flipping to `nil`.
    func removeActiveAccount() {
        guard let active = kernelIdentity.activeAccount else { return }
        kernel.removeAccount(identityId: active)
    }

    // ── Snapshot apply ─────────────────────────────────────────────────────

    private func apply(result: KernelUpdateResult) {
        let update = result.update
        guard update.rev > rev else { return }
        snapshot = update
        // Mirror the identity slice on every accepted tick — the actor is
        // the single writer, and even a tick with no podcast-projection
        // delta may carry fresh identity state (e.g. handshake stage
        // transitions are emitted via the same kernel update loop).
        kernelIdentity = result.identity
        snapshotCount &+= 1
        lastSnapshotAt = Date()
        kmLog.debug("apply rev=\(update.rev) running=\(update.running)")
    }
}
