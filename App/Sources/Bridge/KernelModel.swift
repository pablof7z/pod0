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

    /// Latest download-queue snapshot, updated on every accepted frame where
    /// the downloads value changed. `podcastSnapshot` deliberately excludes
    /// `d.progress` from its content hash to avoid per-second list churn;
    /// this property is the dedicated observation target so `AppStateStore`
    /// can re-run `applyDownloadOverlay` with fresh progress values.
    private(set) var downloadSnapshot: DownloadQueueSnapshot?

    // ── Local counters ─────────────────────────────────────────────────────

    private(set) var snapshotCount: UInt64 = 0
    private(set) var lastSnapshotAt: Date?

    /// Clearable toast text sourced from snapshot or synchronous dispatch
    /// rejections.
    private(set) var lastErrorToast: String?

    /// Mandatory NMP v0.1.0 surface (V-67). Non-nil when the kernel could not
    /// open its on-disk LMDB store and fell back to in-memory — this session's
    /// data will not persist. Carried on every accepted tick (set on failure,
    /// cleared back to `nil` once the store recovers). RootView presents a
    /// user-facing alert; the kernel actor is the sole writer.
    private(set) var storeOpenFailure: String?

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
    /// Monotonic generation bumped each time `library` is reassigned (i.e. on
    /// every `libraryMetaHash` change — see `commitPodcastProjection`). The
    /// kernel-projection pass reads this to detect, in O(1), that a tick fired
    /// purely on a `podcastSnapshot`/`kernelIdentity` change while `library`
    /// stayed byte-identical — letting it skip the O(N) episode rebuild.
    /// `@ObservationIgnored`: callers already observe `library`; a separate
    /// tracked generation would arm a redundant observation.
    @ObservationIgnored private(set) var libraryGeneration: Int = 0
    /// Latest full podcast snapshot (library, player, account …).
    private(set) var podcastSnapshot: PodcastUpdate?
    /// Live player state — updated on every snapshot tick (4 Hz during playback).
    /// Views that only need player position should observe this instead of
    /// `podcastSnapshot?.nowPlaying` so they don't hold a reference to the
    /// full snapshot struct. All other views should use `podcastSnapshot`.
    private(set) var nowPlaying: PlayerState?
    /// Called on the MainActor on every `Playing` audio report with the
    /// episode id string and position (seconds). Wired by `attachKernel` so
    /// `AppStateStore` can forward 1 Hz position ticks into
    /// `setEpisodePlaybackPosition` without relying on `withObservationTracking`.
    var onPositionTick: ((String, Double) -> Void)?
    /// Hash of the library fields that matter to list views. Excludes
    /// `playbackPositionSecs` so list views don't re-render at 4 Hz
    /// during playback (the position is only needed by the player row).
    private var lastLibraryMetaHash: Int = 0
    /// Hash of the snapshot fields that matter to non-player UI. Excludes
    /// `nowPlaying.positionSecs` and `nowPlaying.bufferingFraction` so
    /// views like HomeView, InboxView, etc. don't re-render at 4 Hz.
    private var lastSnapshotContentHash: Int = 0
    /// Rev of the last snapshot we decoded from the kernel. Unlike
    /// `podcastSnapshot?.rev` (which only advances on content changes),
    /// this tracks every processed tick so the short-circuit guards stay
    /// accurate.
    private var lastProcessedRev: UInt64 = 0
    /// Serial queue for the off-MainActor full-library snapshot decode. The
    /// `JSONDecoder` pass measured ~35 ms on the simulator (≈100 ms on device)
    /// at 3.6k episodes — it must never run on the MainActor. Serial so a burst
    /// of rapid dispatches doesn't pile up concurrent multi-MB decodes; the
    /// rev-monotonic guards in `applyPodcastUpdate` drop any stale frame that a
    /// late decode produces. See `docs/perf/ffi-snapshot-transport-findings.md`.
    private let snapshotDecodeQueue = DispatchQueue(
        label: "podcast.snapshot-decode", qos: .userInitiated)

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
        // Prime Rust's is_on_wifi flag before the first feed refresh.
        kernel.startNetworkMonitor()
        // Reactive replacement for the old 500ms poll: shell-initiated FFI
        // reports (audio/download/voice) bump the podcast `rev` without emitting
        // a kernel push frame, so surface them with a one-shot rev-gated pull at
        // the moment they happen. Dispatched host-ops already arrive via the
        // push frame (`apply`) and `dispatch`/`dispatchSilent`'s own pull.
        //
        // Audio reports fire at playback frequency, so this hook is a 4 Hz hot
        // path — the full-library decode and the O(N×M) list hashing both run off
        // the MainActor (see `pullPodcastSnapshotIfChanged`). Live player position
        // still updates inline via `nowPlaying`/`snapshot` at the top of
        // `applyPodcastUpdate`.
        kernel.onSnapshotMaybeChanged = { [weak self] in
            self?.pullPodcastSnapshotIfChanged()
        }
        // Download reports carry their own narrow snapshot inline; progress ticks
        // no longer bump the global `rev`, so they must NOT pull/decode the full
        // library. Update `downloadSnapshot` directly (drives the row overlay) and
        // pull only when durable library state changed (completion/cancellation).
        kernel.onDownloadReport = { [weak self] downloads, durableChanged in
            self?.applyDownloadReport(downloads: downloads, durableChanged: durableChanged)
        }
        // Audio reports carry the fresh player state inline; `Playing`/buffering
        // ticks no longer bump the global `rev`, so they must NOT pull/decode the
        // full library. Update `nowPlaying` + the live media surfaces directly
        // and pull only when structural state changed (play/pause/stop, end).
        kernel.onAudioReport = { [weak self] nowPlaying, durableChanged in
            self?.applyAudioReport(nowPlaying: nowPlaying, durableChanged: durableChanged)
        }
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
        // One-shot startup pull: the persisted library is loaded during register,
        // so it's already in the projection — surface it once. The decode runs
        // off the MainActor (a 3.6k-episode library decode would otherwise block
        // the launch MainActor for ~100 ms); the first frame lands a runloop
        // later, which is imperceptible at launch. Everything after this is
        // event-driven (push frame + report hooks); no timer/poll.
        pullPodcastSnapshotIfChanged()
    }

    func stop() {
        kernel.stop()
        startedKernel = false
    }

    func resetAndRestart() {
        kernel.reset()
        snapshot = nil
        podcastSnapshot = nil
        library = []
        lastProcessedRev = 0
        kernel.reregisterPodcastProjection()
        lastErrorToast = nil
        storeOpenFailure = nil
        kernel.start(visibleLimit: visibleLimit, emitHz: emitHz)
        startedKernel = true
        // One-shot post-reset pull — surface the re-registered projection. Decode
        // runs off the MainActor; the rebuilt UI is current a runloop later.
        pullPodcastSnapshotIfChanged()
    }

    /// One-shot rev-gated pull. This is NOT a poll — there is no timer; the
    /// 500ms background poll has been removed in favor of the reactive push
    /// (`apply(result:)`).
    ///
    /// The full-library `JSONDecoder` pass (`kernel.podcastSnapshot()`) is the
    /// expensive step — ~35 ms on the simulator (≈100 ms on device) at 3.6k
    /// episodes — and it ALWAYS runs off the MainActor now, on
    /// `snapshotDecodeQueue`. Previously it ran inline on the MainActor for every
    /// user dispatch (mark-played, star, subscribe, …), so each action ate that
    /// decode as a main-thread stall — the "sluggish" the user reported. Moving
    /// it off-main is safe because no caller reads `library`/`podcastSnapshot`/
    /// `episodes` synchronously after `dispatch()` returns: every dispatch site is
    /// fire-and-forget over `@Observable`, so a one-runloop-later commit is
    /// invisible. This brings the pull path in line with the push path, whose
    /// decode already runs off the MainActor (on the kernel C-callback thread).
    ///
    /// Ordering: rapid pulls may enqueue several decodes; the rev-monotonic
    /// guards in `applyPodcastUpdate` (`update.rev > lastProcessedRev`) and
    /// `commitPodcastProjection` (`frameRev == lastProcessedRev`) make the newest
    /// frame win and drop any stale one. The decode is dispatched off-main even
    /// for the `start` / `resetAndRestart` one-shots (a one-runloop-later first
    /// frame is imperceptible and keeps a 3.6k-episode decode off the launch
    /// MainActor); the O(N×M) hashing then also runs off-main inside
    /// `applyPodcastUpdate`.
    ///
    /// `synchronous` is accepted but IGNORED — the decode is always off-main now.
    /// It is retained only for source-compatibility with the in-flight audio
    /// rev-discipline PR (#265), whose `applyAudioReport` calls this with
    /// `synchronous: false`; keeping the defaulted parameter lets the two PRs
    /// compose in either merge order without a compile break. Remove it (and that
    /// call site's argument) in a follow-up once both have landed.
    private func pullPodcastSnapshotIfChanged(synchronous: Bool = false) {
        let currentRev = kernel.podcastSnapshotRev()
        guard currentRev > lastProcessedRev else { return }
        let handle = kernel
        snapshotDecodeQueue.async { [weak self] in
            let update = handle.podcastSnapshot()
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    self?.applyPodcastUpdate(update)
                }
            }
        }
    }

    /// Apply a download-report response (from `attachDownloadReportChannel`).
    ///
    /// Progress ticks (~1 Hz per active download) land here and update only the
    /// always-fresh `downloadSnapshot` — the source `AppStateStore`'s row
    /// overlay reads. They do NOT bump the global `rev` in Rust, so they never
    /// pull or JSON-decode the full library snapshot (the empirical CPU/heat
    /// hot path). Only a durable change (completion/cancellation, which flips
    /// `Episode.downloadState`) sets `durableChanged`; then we pull the full
    /// snapshot so the library projection reprojects the affected episode.
    @MainActor
    func applyDownloadReport(downloads: DownloadQueueSnapshot?, durableChanged: Bool) {
        if downloads != downloadSnapshot {
            downloadSnapshot = downloads
        }
        if durableChanged {
            pullPodcastSnapshotIfChanged()
        }
    }

    /// Apply one audio report's inline player state. The hot path: `Playing`
    /// (≤4 Hz playhead) and `BufferingProgress` ticks arrive here with
    /// `durableChanged == false`, so they refresh ONLY the live surfaces
    /// (`nowPlaying` scrubber + Dynamic Island + lock-screen elapsed) using the
    /// already-decoded `library` — never re-decoding the 3k-episode snapshot. A
    /// structural report (play/pause/stop, track end, sleep-timer) additionally
    /// pulls the full snapshot so list-view state stays correct.
    ///
    /// Mirrors the `nowPlaying`/reconcile block of `applyPodcastUpdate` (the
    /// path durable reports still take), minus the library decode + hashing.
    func applyAudioReport(nowPlaying newNowPlaying: PlayerState?, durableChanged: Bool) {
        let previous = nowPlaying
        nowPlaying = newNowPlaying
        // Forward position to AppStateStore so the debounce cache stays current.
        // Covers Playing, BufferingProgress (which advances positionSecs with
        // isPlaying=false), and the final Paused frame (capturing the last
        // playhead before a force-quit). Guard only on positionSecs > 0 and
        // episodeId being present; skips stopped/reset states automatically.
        if let np = newNowPlaying, np.positionSecs > 0, let id = np.episodeId,
           !np.didReachNaturalEnd {
            onPositionTick?(id, np.positionSecs)
        }
        // Live media surfaces, off the library-decode path. `reconcileLiveActivity`
        // coalesces same-episode position updates; `reconcileNowPlayingMetadata`
        // is a no-op unless the episode changed — both cheap, and `library` is
        // the current cached value (unchanged by a position tick).
        reconcileLiveActivity(previous: previous, next: newNowPlaying, library: library)
        reconcileNowPlayingMetadata(previous: previous, next: newNowPlaying, library: library)
        // Always probe — but `pullPodcastSnapshotIfChanged` is rev-gated, and
        // since `Playing`/buffering ticks no longer bump the global `rev`, a tick
        // with no other activity costs only one atomic read (no decode, no
        // rebuild). This intentionally preserves the reactive side-channel the
        // per-tick pull used to provide: background actor-thread work that bumps
        // `rev` off the kernel emit path (inbox triage, categorization, and any
        // tokio-spawned projection update) still reaches the UI during a long
        // listen. A real change — a durable audio event OR a background bump —
        // advances `rev` and triggers exactly one full rebuild; `durableChanged`
        // is informational (the rev gate, not the flag, decides the pull).
        pullPodcastSnapshotIfChanged(synchronous: false)
    }

    /// Apply one `PodcastUpdate` to the observable surface. Shared by the
    /// reactive push path (`apply(result:)`) and the rev-gated pull
    /// (`pullPodcastSnapshotIfChanged`). Rev-gated so redundant frames (push at
    /// emit-Hz, or a pull racing a push) are dropped cheaply.
    ///
    /// `update` is ALWAYS produced off the MainActor now — the push path decodes
    /// on the kernel C-callback thread, the pull path on `snapshotDecodeQueue`.
    /// This method runs the cheap, must-be-main work inline (the `@Observable`
    /// `snapshot`/`nowPlaying`/`downloadSnapshot` assignments + Spotlight / Live
    /// Activity / Now-Playing reconcile) and then offloads the O(N×M) content/
    /// library hashing to a detached task, committing `podcastSnapshot`/`library`
    /// back on the MainActor. There is no longer a "synchronous" same-runloop
    /// path: no caller reads `library`/`podcastSnapshot`/`episodes` synchronously
    /// after a dispatch (every dispatch site is fire-and-forget over
    /// `@Observable`), so the list-view properties always land a hop later, which
    /// SwiftUI observation tolerates. `nowPlaying`/`snapshot` (live player
    /// position) are still assigned inline at the top so the player surface stays
    /// current the moment a frame is accepted.
    private func applyPodcastUpdate(_ update: PodcastUpdate) {
        guard update.rev > lastProcessedRev else { return }
        lastProcessedRev = UInt64(update.rev)
        snapshot = update
        if update.downloads != downloadSnapshot { downloadSnapshot = update.downloads }
        let previousNowPlaying = nowPlaying
        // `nowPlaying` carries live playback position; refresh on every accepted
        // frame so player views stay current. Stays on the MainActor inline so
        // the live player surface reflects the frame the moment it's accepted.
        nowPlaying = update.nowPlaying
        PodcastCapabilities.shared.iCloudSync.applySettingsSnapshot(
            SettingsKVSnapshot.from(podcastUpdate: update))
        PodcastCapabilities.shared.spotlight.indexLibrary(update.library)
        reconcileLiveActivity(
            previous: previousNowPlaying, next: update.nowPlaying, library: update.library)
        reconcileNowPlayingMetadata(
            previous: previousNowPlaying, next: update.nowPlaying, library: update.library)
        kmLog.debug("podcast update rev=\(update.rev) library=\(update.library.count)")

        // Gate `podcastSnapshot` (and `library`) on content hashes that exclude
        // volatile position/buffering fields so list views don't re-render at
        // the emit rate. Both hashes are O(N×M) (every show × every episode ×
        // multiple fields), so the *computation* is offloaded to a detached task;
        // only the cheap comparison and the `@Observable` assignments come back
        // on the MainActor.
        let frameRev = UInt64(update.rev)
        Task.detached(priority: .utility) { [weak self] in
            guard let self else { return }
            let snapHashInterval = signposter.beginInterval("snapshotContentHash")
            let newSnapHash = self.snapshotContentHash(for: update)
            signposter.endInterval("snapshotContentHash", snapHashInterval)
            let libHashInterval = signposter.beginInterval("libraryMetaHash")
            let newLibHash = self.libraryMetaHash(for: update.library)
            signposter.endInterval("libraryMetaHash", libHashInterval)
            await MainActor.run {
                self.commitPodcastProjection(
                    update: update, frameRev: frameRev,
                    newSnapHash: newSnapHash, newLibHash: newLibHash)
            }
        }
    }

    /// Commit the rev-gated `podcastSnapshot`/`library` assignments. Shared by
    /// both the inline (pull) and detached (push) hashing paths so they can
    /// never drift. The `frameRev == lastProcessedRev` reentrancy guard is
    /// load-bearing for the async path — 4 Hz hops interleave, so a
    /// late-returning stale frame must not clobber newer state; `lastProcessedRev`
    /// is monotonic, so a newer frame already advanced it (newest wins). On the
    /// synchronous path the guard is trivially true (nothing ran between
    /// assigning `lastProcessedRev` above and arriving here).
    private func commitPodcastProjection(
        update: PodcastUpdate, frameRev: UInt64, newSnapHash: Int, newLibHash: Int
    ) {
        guard frameRev == lastProcessedRev else { return }
        if newSnapHash != lastSnapshotContentHash {
            lastSnapshotContentHash = newSnapHash
            podcastSnapshot = update
        }
        if newLibHash != lastLibraryMetaHash {
            lastLibraryMetaHash = newLibHash
            library = update.library
            // Bump AFTER the assignment so a reader that samples the generation
            // alongside `library` sees them advance together.
            libraryGeneration &+= 1
        }
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
            // Cold start skips `RefreshAll` (the snapshot already loaded from
            // disk), so the fresh-feed auto-download path never runs at launch.
            // Kick a catch-up evaluation over the current library so enabled
            // shows still pull their latest undownloaded episodes without
            // waiting for a manual pull-to-refresh.
            _ = dispatch(namespace: "podcast", body: ["op": "auto_download_evaluate"])
            return
        }
        dispatch(PodcastKernelAction.RefreshAll())
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
        // Surface the result of the user action without waiting for the next
        // push frame. The full-library decode runs off the MainActor (see
        // `pullPodcastSnapshotIfChanged`) so this dispatch returns immediately;
        // the projection commits a runloop later — no caller depends on a
        // same-runloop read of `library`/`podcastSnapshot`/`episodes`.
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
        // Surface the result off-main, same as `dispatch`.
        pullPodcastSnapshotIfChanged()
        return result
    }

    // ── Transcript report ───────────────────────────────────────────────

    /// Report a completed transcript to the Rust kernel so AI features
    /// can access the plain text without going through Swift's TranscriptStore.
    func sendTranscriptReport(episodeID: UUID, text: String) {
        guard let handle = kernel.podcastHandle else { return }
        let payload: [String: Any] = [
            "episode_id": episodeID.uuidString,
            "text": text
        ]
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return }
        jsonStr.withCString { ptr in
            let result = nmp_app_podcast_transcript_report(handle, ptr)
            if let result { nmp_app_free_string(result) }
        }
    }

    // ── Episode pipeline event log (Diagnostics) ────────────────────────

    /// Fetch the kernel's per-episode pipeline event log (download / transcript
    /// / identify lifecycle). A small, synchronous single-episode FFI read —
    /// the events deliberately do NOT ride the library snapshot, so the
    /// Diagnostics sheet pulls them lazily on appear and on the snapshot
    /// generation changes it already observes. Returns `[]` when the kernel is
    /// unregistered, the episode has no log, or the payload fails to decode.
    func fetchEpisodeEvents(episodeID: UUID) -> [EpisodeAuditEvent] {
        guard let handle = kernel.podcastHandle else { return [] }
        return episodeID.uuidString.withCString { ptr -> [EpisodeAuditEvent] in
            guard let result = nmp_app_podcast_episode_events(handle, ptr) else { return [] }
            defer { nmp_app_free_string(result) }
            guard let data = String(cString: result).data(using: .utf8) else { return [] }
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            return (try? decoder.decode([EpisodeAuditEvent].self, from: data)) ?? []
        }
    }

    // ── Provider-blind LLM chat completion ──────────────────────────────

    /// Resolve the opaque podcast handle pointer for use in a blocking FFI call.
    /// The pointer is stable for the process lifetime once registered (D6).
    /// Returns nil when the kernel is not yet registered.
    var podcastHandlePointer: UnsafeMutableRawPointer? {
        kernel.podcastHandle
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

    /// Generate a fresh account in the kernel (keypair + kind:0 publish). The
    /// kernel owns the secret; Swift never holds private bytes. When
    /// `makeActive` is true the new account becomes the active session and its
    /// pubkey arrives on the next snapshot tick via
    /// `kernelIdentity.activeAccount`. `profile` is a flat string-map and
    /// `relays` is a list of `[url, role]` pairs; both default to kernel
    /// defaults when omitted.
    func createNewAccount(
        profile: [String: String] = [:],
        relays: [[String]] = [],
        mls: Bool = false,
        makeActive: Bool = true
    ) {
        let profileJSON = (try? JSONSerialization.data(withJSONObject: profile, options: [.sortedKeys]))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        let relaysJSON = (try? JSONSerialization.data(withJSONObject: relays, options: []))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
        kernel.createNewAccount(
            profileJSON: profileJSON,
            relaysJSON: relaysJSON,
            mls: mls,
            makeActive: makeActive
        )
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

    /// Sign an unsigned NIP-01 event draft through the kernel (D13 — no private
    /// key in Swift) and await the resulting flat wire-event JSON. `accountPubkeyHex`
    /// empty selects the active account. Forwards to the `PodcastHandle`
    /// sign-for-return seam, which resolves against the drain-once
    /// `signed_events` projection.
    func signEventForReturn(accountPubkeyHex: String, unsignedJSON: String) async throws -> String {
        try await kernel.signEventForReturn(
            accountPubkeyHex: accountPubkeyHex, unsignedJSON: unsignedJSON)
    }

    // ── Profile resolution (reference-first; rides resolved_profiles) ──────
    //
    // Replaces the host opening its own websocket to fetch kind:0. A view that
    // displays a Nostr profile claims the pubkey on appear and releases on
    // disappear; the kernel fetches kind:0 over its own relay pool and delivers
    // the result via `projections.resolved_profiles`, which
    // `AppStateStore.mergeResolvedProfiles` folds into `nostrProfileCache`. The
    // display then re-renders reactively. `consumerID` is a stable per-view
    // token so the kernel's refcount dedupes and release matches claim.

    /// Claim a refcounted interest in `pubkeyHex`'s kind:0 profile.
    func claimProfile(pubkeyHex: String, consumerID: String) {
        kernel.claimProfile(pubkeyHex: pubkeyHex, consumerID: consumerID)
    }

    /// Release a previously-claimed profile interest.
    func releaseProfile(pubkeyHex: String, consumerID: String) {
        kernel.releaseProfile(pubkeyHex: pubkeyHex, consumerID: consumerID)
    }

    // ── Snapshot apply ─────────────────────────────────────────────────────

    private func apply(result: KernelUpdateResult) {
        // Perf: time the synchronous main-actor segment of every accepted push
        // frame (identity diff, spotlight index, live-activity + now-playing
        // reconcile). The O(N×M) hashing it kicks off is offloaded off-main, so
        // this measures only the inline main-thread cost. Recorded via `defer`
        // so it covers every return path.
        let applyStart = DispatchTime.now().uptimeNanoseconds
        defer {
            PerfMetrics.shared.record(
                .mainApply,
                micros: Int((DispatchTime.now().uptimeNanoseconds &- applyStart) / 1_000))
        }
        // The store-open-failure health flag and the identity slice are
        // independent of the podcast projection rev — assign them on every
        // accepted push frame (before the rev-gated podcast-state apply) so the
        // mandatory store alert fires on the first frame and identity stays live
        // even on ticks where the podcast projection didn't change.
        storeOpenFailure = result.storeOpenFailure
        // `apply` runs on every accepted push frame (4 Hz during playback), but
        // the identity slice — accounts, handshake, and the resolved-profiles
        // map — changes far less often. Gate the `@Observable` write on a real
        // change so identity observers (and the resolved-profiles → cache pump
        // in `applyKernelState`) don't fire a full projection rebuild at the
        // emit rate. Relies on `KernelIdentityProjection: Equatable`.
        if result.identity != kernelIdentity {
            kernelIdentity = result.identity
        }
        snapshotCount &+= 1
        lastSnapshotAt = Date()
        // Drive the podcast observable surface from the pushed projection. The
        // frame was already decoded off the MainActor (C-callback thread); the
        // O(N×M) hashing is then also offloaded off-main inside applyPodcastUpdate.
        applyPodcastUpdate(result.update)
    }
}
