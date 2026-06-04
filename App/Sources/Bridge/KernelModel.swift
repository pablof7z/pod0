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
        // `synchronous: false`: audio reports fire at playback frequency, so this
        // hook is a 4 Hz hot path — keep the O(N×M) list hashing off the
        // MainActor. There is no same-runloop freshness contract here (no user
        // action is waiting on it); live player position still updates inline via
        // `nowPlaying`/`snapshot` at the top of `applyPodcastUpdate`.
        kernel.onSnapshotMaybeChanged = { [weak self] in
            self?.pullPodcastSnapshotIfChanged(synchronous: false)
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
        // One-shot startup pull: the persisted library is loaded synchronously
        // during register, so it's already in the projection — surface it once,
        // synchronously, so the first frame is on-screen immediately on launch.
        // Everything after this is event-driven (push frame + report hooks); no
        // timer/poll.
        pullPodcastSnapshotIfChanged(synchronous: true)
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
        // One-shot post-reset pull — surface the re-registered projection
        // synchronously so the rebuilt UI is current immediately.
        pullPodcastSnapshotIfChanged(synchronous: true)
    }

    /// One-shot rev-gated pull. This is NOT a poll — there is no timer; the
    /// 500ms background poll has been removed in favor of the reactive push
    /// (`apply(result:)`).
    ///
    /// `synchronous` is threaded straight through to `applyPodcastUpdate` and
    /// follows the same "is there a synchronous freshness contract?" rule:
    ///
    /// - `true` — user-action callers (`dispatch` / `dispatchSilent`) and the
    ///   one-shot startup pulls (`start` / `resetAndRestart`). The action must be
    ///   reflected in the same runloop pass, so the hashing runs inline on the
    ///   MainActor and `library`/`podcastSnapshot` are assigned before the
    ///   dispatch returns.
    /// - `false` — the reactive report hook (`onSnapshotMaybeChanged`, fed by the
    ///   audio/download/voice FFI report channels). Audio reports fire at
    ///   playback frequency, so running the O(N×M) `libraryMetaHash` inline here
    ///   would reintroduce the very main-thread cost this change removes. There
    ///   is no same-runloop contract for these — `nowPlaying`/`snapshot` (which
    ///   carry live position) are still assigned synchronously at the top of
    ///   `applyPodcastUpdate`; only the list-view properties hop off-main.
    private func pullPodcastSnapshotIfChanged(synchronous: Bool) {
        let currentRev = kernel.podcastSnapshotRev()
        guard currentRev > lastProcessedRev else { return }
        applyPodcastUpdate(kernel.podcastSnapshot(), synchronous: synchronous)
    }

    /// Apply one `PodcastUpdate` to the observable surface. Shared by the
    /// reactive push path (`apply(result:)`) and the rev-gated pull
    /// (`pullPodcastSnapshotIfChanged`). Rev-gated so redundant frames (push at
    /// emit-Hz, or a pull racing a push) are dropped cheaply.
    ///
    /// `synchronous` selects where the O(N×M) content/library hashing runs. The
    /// rule is "is a caller waiting on same-runloop freshness?", NOT "push vs
    /// pull" — the pull funnel serves both a user-action path and a
    /// playback-frequency report hook, so the flag is decided per call site:
    ///
    /// - `false` (no freshness contract): the 4 Hz push frame (`apply(result:)`)
    ///   and the reactive report hook (`onSnapshotMaybeChanged`, which audio
    ///   reports fire at playback frequency). The hash *computation* is offloaded
    ///   to a detached utility task so these hot paths don't pay it on the
    ///   MainActor. `podcastSnapshot`/`library` land a hop later, which SwiftUI
    ///   observation tolerates.
    /// - `true` (same-runloop contract): user-action dispatch
    ///   (`dispatch`/`dispatchSilent`) and the one-shot startup pulls
    ///   (`start`/`resetAndRestart`). The hashing runs inline on the MainActor so
    ///   `podcastSnapshot`/`library` are assigned *before this call returns* —
    ///   preserving the same-runloop guarantee a user action depends on.
    ///
    /// `nowPlaying`/`snapshot` (live player position) are assigned synchronously
    /// at the top of this method on *every* path, so the flag only governs the
    /// list-view properties.
    private func applyPodcastUpdate(_ update: PodcastUpdate, synchronous: Bool) {
        guard update.rev > lastProcessedRev else { return }
        lastProcessedRev = UInt64(update.rev)
        snapshot = update
        if update.downloads != downloadSnapshot { downloadSnapshot = update.downloads }
        let previousNowPlaying = nowPlaying
        // `nowPlaying` carries live playback position; refresh on every accepted
        // frame so player views stay current. Stays on the MainActor inline so
        // the live player surface (and post-dispatch pull) reflects same-runloop.
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
        // multiple fields).
        let frameRev = UInt64(update.rev)
        if synchronous {
            // Pull path: compute inline on the MainActor so the assignments are
            // visible in the same runloop pass the dispatch returned in.
            let snapHashInterval = signposter.beginInterval("snapshotContentHash")
            let newSnapHash = snapshotContentHash(for: update)
            signposter.endInterval("snapshotContentHash", snapHashInterval)
            let libHashInterval = signposter.beginInterval("libraryMetaHash")
            let newLibHash = libraryMetaHash(for: update.library)
            signposter.endInterval("libraryMetaHash", libHashInterval)
            commitPodcastProjection(
                update: update, frameRev: frameRev,
                newSnapHash: newSnapHash, newLibHash: newLibHash)
        } else {
            // Push-frame path: the two hashes previously ran on the MainActor on
            // every accepted 4 Hz push frame during playback — before the gate
            // could discard the frame. Move the *computation* off the MainActor;
            // only the cheap comparison and the `@Observable` assignments come
            // back on.
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
        // Synchronous: a user action must be reflected in the same runloop pass
        // rather than waiting for the next 4 Hz push frame.
        pullPodcastSnapshotIfChanged(synchronous: true)
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
        // Synchronous: same same-runloop contract as `dispatch`.
        pullPodcastSnapshotIfChanged(synchronous: true)
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
        // Drive the podcast observable surface from the pushed projection.
        // Async: no synchronous freshness contract on the 4 Hz push path, so the
        // O(N×M) hashing is offloaded off the MainActor.
        applyPodcastUpdate(result.update, synchronous: false)
    }
}
