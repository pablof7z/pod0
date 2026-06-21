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
    // widened to internal so KernelModel+Reports can write on download ticks
    var downloadSnapshot: DownloadQueueSnapshot?

    // ── Local counters ─────────────────────────────────────────────────────

    private(set) var snapshotCount: UInt64 = 0
    private(set) var lastSnapshotAt: Date?

    /// Clearable toast text sourced from snapshot or synchronous dispatch
    /// rejections.
    // widened to internal so KernelModel+Dispatch can clear/set it
    var lastErrorToast: String?

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
    // widened to internal so KernelModel+Reports can update on audio ticks
    var nowPlaying: PlayerState?
    /// Called on the MainActor on every `Playing` audio report with the
    /// episode id string and position (seconds). Wired by `attachKernel` so
    /// UI consumers (scrubber, Live Activity) receive live position ticks
    /// without relying on `withObservationTracking`.
    var onPositionTick: ((String, Double) -> Void)?
    /// Called on the MainActor when Rust completes an agent-ask lifecycle
    /// event asynchronously, currently timeout expiry.
    var onAgentAskEvent: ((AgentAskResponse) -> Void)?
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
    /// `false` until the first cold-start full pull has been successfully
    /// applied. Guards the re-seed allowance: before the first hydration
    /// the pull path uses `>=` instead of `>` when comparing
    /// `update.rev` against `lastProcessedRev`, so a partial push frame
    /// that races the startup pull and advances `lastProcessedRev` cannot
    /// permanently block the full-library snapshot from seeding the
    /// composite. Flipped to `true` inside `applyPodcastUpdate` the
    /// moment the first `fromPull` frame commits; after that the
    /// steady-state `>` guard is restored for both push and pull paths.
    private(set) var hasHydratedPodcastSnapshot: Bool = false
    /// Per-domain last-applied rev counters. Each domain frame's `rev` is
    /// compared here before merging — stale/duplicate frames are dropped
    /// without touching the composite.
    private var domainRevTracker = DomainRevTracker()
    /// Composite `PodcastUpdate` — the current merged state built by
    /// selectively replacing domains as per-domain push frames arrive. The
    /// pull path replaces the entire composite on cold-start / fallback.
    private var compositeUpdate: PodcastUpdate = PodcastUpdate()
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

    let kernel = PodcastHandle() // internal: extension files dispatch through it
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
        kernel.attachHttpReportChannel()
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
        kernel.onAgentAskEvent = { [weak self] response in
            self?.onAgentAskEvent?(response)
        }
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
        hasHydratedPodcastSnapshot = false
        domainRevTracker = DomainRevTracker()
        compositeUpdate = PodcastUpdate()
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
    /// `synchronous` is retained for source compatibility; decode is always
    /// off-main.
    // internal (not private) so extension files can trigger snapshot pulls.
    func pullPodcastSnapshotIfChanged(synchronous: Bool = false) {
        let currentRev = kernel.podcastSnapshotRev()
        guard Self.shouldPullPodcastSnapshot(
            currentRev: currentRev,
            lastProcessedRev: lastProcessedRev,
            hasHydratedPodcastSnapshot: hasHydratedPodcastSnapshot
        ) else { return }
        let handle = kernel
        snapshotDecodeQueue.async { [weak self] in
            let update = handle.podcastSnapshot()
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    // Pull path always replaces the composite so push merges
                    // start from the current full state (fromPull: true).
                    self?.applyPodcastUpdate(update, fromPull: true)
                }
            }
        }
    }

    /// Apply one `PodcastUpdate` to the observable surface. Shared by:
    ///   - The per-domain push path (`apply(result:)` → `mergeDomainFrames`)
    ///   - The rev-gated pull path (`pullPodcastSnapshotIfChanged`)
    ///
    /// Rev-gated so redundant frames (push at emit-Hz, or a pull racing a push)
    /// are dropped cheaply. For the push path `update` is the already-merged
    /// `compositeUpdate`; for the pull path it is the full library snapshot.
    ///
    /// This method runs the cheap, must-be-main work inline (the `@Observable`
    /// `snapshot`/`nowPlaying`/`downloadSnapshot` assignments + Spotlight / Live
    /// Activity / Now-Playing reconcile) and then offloads the O(N×M) content/
    /// library hashing to a detached task, committing `podcastSnapshot`/`library`
    /// back on the MainActor.
    ///
    /// `fromPull`: when true, also replace `compositeUpdate` with the full
    /// snapshot so the push path's incremental merges start from a current base.
    private func applyPodcastUpdate(_ update: PodcastUpdate, fromPull: Bool = false) {
        // Cold-start re-seed allowance: before the first full hydration a partial
        // push frame may have already advanced `lastProcessedRev` to the same rev
        // the startup pull carries. Allow `>=` on the cold-start pull so the full
        // library snapshot still seeds the composite even if a partial push frame
        // raced it. After `hasHydratedPodcastSnapshot` flips true the normal `>` guard is
        // restored for all subsequent push and pull frames.
        let revPasses = fromPull && !hasHydratedPodcastSnapshot
            ? update.rev >= Int(lastProcessedRev)
            : update.rev > Int(lastProcessedRev)
        guard revPasses else { return }
        lastProcessedRev = UInt64(update.rev)
        // For the pull path, replace the composite so future push merges start
        // from the current full state rather than a stale domain-by-domain build.
        if fromPull {
            compositeUpdate = update
            hasHydratedPodcastSnapshot = true
        }
        snapshot = update
        if update.downloads != downloadSnapshot { downloadSnapshot = update.downloads }
        let previousNowPlaying = nowPlaying
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
        // the emit rate. Both hashes are O(N×M) — offloaded off-main.
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
    // widened to internal so KernelModel+Dispatch can write it in lifecycleForeground
    var hasObservedForeground = false

    // ── Snapshot apply ─────────────────────────────────────────────────────

    private func apply(result: KernelUpdateResult) {
        // Perf: time the synchronous main-actor segment of every accepted push
        // frame. The O(N×M) hashing it kicks off is offloaded off-main.
        let applyStart = DispatchTime.now().uptimeNanoseconds
        defer {
            PerfMetrics.shared.record(
                .mainApply,
                micros: Int((DispatchTime.now().uptimeNanoseconds &- applyStart) / 1_000))
        }
        // Store-open-failure fires on every frame (before rev-gating) so the
        // mandatory store alert fires on the first frame (V-67).
        storeOpenFailure = result.storeOpenFailure
        // Identity: only replace when the identity domain sidecar was present in
        // this frame (result.identity != .empty). Absent = identity unchanged;
        // preserve the current kernelIdentity rather than clobbering with .empty.
        // This is correct because the kernel only emits the identity sidecar when
        // the identity domain rev advanced (sign-in, sign-out, account change).
        if result.identity != .empty {
            if result.identity != kernelIdentity {
                kernelIdentity = result.identity
            }
        } else if !result.domainFrames.resolvedProfiles.isEmpty {
            // `resolved_profiles` is a TOP-LEVEL projections key (not a
            // `podcast.*` domain sidecar) that can arrive on any tick —
            // including ticks where the identity domain sidecar is absent
            // (result.identity == .empty). Merge the new profiles additively
            // into the cached kernelIdentity so the consumer (AppStateStore
            // → mergeResolvedProfiles) receives them on the next observation
            // tick without clobbering the active-account fields.
            let merged = kernelIdentity.merging(
                resolvedProfiles: result.domainFrames.resolvedProfiles)
            if merged != kernelIdentity {
                kernelIdentity = merged
            }
        }
        snapshotCount &+= 1
        lastSnapshotAt = Date()

        // Merge each present domain into the composite and flow through
        // applyPodcastUpdate only when at least one domain was accepted.
        // The drop-guard inside mergeDomainFrames handles stale/duplicate frames.
        let accepted = mergeDomainFrames(
            result.domainFrames,
            into: &compositeUpdate,
            tracker: &domainRevTracker)
        guard accepted else { return }

        // Advance the composite rev to the max accepted domain rev so
        // applyPodcastUpdate's rev-monotonic guard lets it through. Use the
        // highest rev across all present domains.
        let maxDomainRev = maxRev(result.domainFrames)
        if compositeUpdate.rev < Int(maxDomainRev) {
            compositeUpdate.rev = Int(maxDomainRev)
        }
        // The kernel is running whenever it emits domain frames — set the flag
        // so `isRunning` stays accurate without a full snapshot pull.
        compositeUpdate.running = true

        // Pass fromPull: false — the composite is already up-to-date; we do NOT
        // want to overwrite it with the full pull snapshot.
        applyPodcastUpdate(compositeUpdate, fromPull: false)
    }

    /// Highest `rev` across all domain frames present in the push frame.
    private func maxRev(_ frames: PodcastDomainFrames) -> UInt64 {
        [frames.library?.rev, frames.playback?.rev, frames.downloads?.rev,
         frames.settings?.rev, frames.identity?.rev, frames.widget?.rev,
         frames.misc?.rev].compactMap { $0 }.max() ?? 0
    }
}
