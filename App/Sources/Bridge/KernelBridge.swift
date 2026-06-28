import Darwin
import Foundation
import os.log

let kbLog = Logger(subsystem: "io.f7z.podcast", category: "KernelBridge")

/// Mirror of the kernel's `schema_version` (Rust: `nmp_core::SNAPSHOT_SCHEMA_VERSION`),
/// emitted on every `PodcastUpdate` projection. Must be bumped in lock-step when the
/// Rust constant changes; snapshot decoding fails closed on a mismatch (#356) rather
/// than silently misparsing a newer/older schema.
let KERNEL_SCHEMA_VERSION = 1

/// Thin C-FFI wrapper around the `nmp_app_podcast` static library.
final class PodcastHandle: @unchecked Sendable {
    let raw: UnsafeMutableRawPointer
    private var updateSink: KernelUpdateSink?
    /// Opaque handle returned by `nmp_app_podcast_register`.
    var podcastHandle: UnsafeMutableRawPointer?
    /// Retained bridge passed as `context` to `nmp_app_set_capability_callback`.
    var syncBridge: SyncCapabilityBridge?
    /// Fired (on the main actor) immediately after a shell-initiated FFI report
    /// (`nmp_app_podcast_audio_report` / `_download_report`) bumps the podcast
    /// `rev`. `KernelModel` wires this to a one-shot rev-gated pull so those
    /// changes reach the UI reactively — replacing the old 500ms snapshot poll.
    /// (Dispatched host-ops already arrive via the kernel push frame.)
    var onSnapshotMaybeChanged: (() -> Void)?

    /// Fired (on the main actor) for every download report, carrying the fresh
    /// `DownloadQueueSnapshot` (progress %, queue state) and whether the report
    /// changed *durable* library state. `KernelModel` wires this to update its
    /// live `downloadSnapshot` directly — progress ticks update only that
    /// (driving the row overlay) WITHOUT pulling/decoding the full library; only
    /// `durableChanged == true` (a completion/cancellation) triggers a full pull.
    /// This is the seam that keeps ~1 Hz progress off the global-`rev` hot path.
    var onDownloadReport: ((DownloadQueueSnapshot?, Bool) -> Void)?

    /// Fired (on the main actor) for every audio report, carrying the fresh
    /// `PlayerState` (live playhead / buffer / play state) and whether the
    /// report changed *structural* state. `KernelModel` wires this to update its
    /// live `nowPlaying` (scrubber, Dynamic Island, lock screen) directly —
    /// `Playing`/`BufferingProgress` ticks update only that WITHOUT
    /// pulling/decoding the full library; only `durableChanged == true`
    /// (play/pause/stop, track end, sleep-timer) triggers a full pull. This is
    /// the seam that keeps ~1 Hz playback ticks off the global-`rev` hot path.
    var onAudioReport: ((PlayerState?, Bool) -> Void)?
    /// Fired on the main actor when Rust completes an agent-ask lifecycle event
    /// asynchronously. Today that is the Rust-owned timeout expiry path.
    var onAgentAskEvent: ((KernelModel.AgentAskResponse) -> Void)?

    /// Buffered resolver for `nmp_app_sign_event_for_return` round-trips. The
    /// `signed_events` projection that carries each result is drain-once: the
    /// kernel clears it on the first emit tick that carries it. Because the
    /// correlation id is only known AFTER the synchronous FFI return, a slow
    /// `await` could miss that single frame. This registry closes the race by
    /// retaining every drained result keyed by id, so any future caller
    /// resolves via find-or-register regardless of thread timing (D13: signing
    /// is the kernel's job, the host never holds a private key).
    let signedEventsRegistry = SignedEventsRegistry()

    /// Buffered resolver for async-completing kernel actions. The
    /// `action_results` projection carries the settled result (e.g. a
    /// `BlobDescriptor` from `nmp.blossom.upload`) as a drained array in each
    /// push frame. `BlossomKernelUploader` awaits its correlation-id here.
    let actionResultsRegistry = ActionResultsRegistry()

    init() {
        raw = nmp_app_new()
        // Register the NIP-46 bunker hook BEFORE any sign-in attempt routes
        // through `nmp_app_signin_bunker`. The broker captures the actor
        // sender immediately; subsequent `bunker://` URIs are silently
        // dropped without this call (D6).
        nmp_signer_broker_init(raw)
        Self.configureStoragePath(for: raw)
        // ADR-0053 / NMP v0.8: make the full built-in projection consumption
        // intent explicit before `nmp_app_start`; podcast domain sidecars are
        // registered separately through `nmp_app_podcast_register`.
        nmp_app_consume_all_builtin_projections(raw)
        registerPodcastProjection()
    }

    private static func configureStoragePath(for raw: UnsafeMutableRawPointer) {
        guard let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask).first
        else { return }
        let directory = base.appendingPathComponent("NMP", isDirectory: true)
        do {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true)
            directory.path.withCString { nmp_app_set_storage_path(raw, $0) }
        } catch {
            kbLog.error(
                "failed to create NMP storage directory: \(error.localizedDescription, privacy: .public)")
        }
    }

    deinit {
        unregisterPodcastProjectionIfNeeded()
        nmp_app_set_update_callback(raw, nil, nil)
        nmp_app_free(raw)
    }

    /// Wire the Rust update callback. `handler` runs on every snapshot frame;
    /// `onPanic` runs exactly once if/when the actor thread dies and the Rust
    /// supervisor emits an `{"t":"panic",...}` envelope (D7 actor-death contract).
    func listen(
        _ handler: @escaping (KernelUpdateResult) -> Void,
        onPanic: @escaping () -> Void = {}
    ) {
        let sink = KernelUpdateSink(
            handler: handler, onPanic: onPanic,
            signedEvents: signedEventsRegistry,
            actionResults: actionResultsRegistry)
        updateSink = sink
        nmp_app_set_update_callback(
            raw, Unmanaged.passUnretained(sink).toOpaque(), nmpUpdateCallback)
    }

    /// Actor-liveness probe (D7 pull-side, ADR-0028). Returns `true` when the
    /// Rust actor thread is still running, `false` when terminated.
    func isAlive() -> Bool {
        nmp_app_is_alive(raw) == 1
    }

    func start(visibleLimit: UInt32 = 80, emitHz: UInt32 = 4) {
        // Set the podcast library data directory here rather than at
        // registerPodcastProjection() time. `nmp_app_podcast_set_data_dir`
        // reads podcasts.json immediately; deferring the call to `start()`
        // ensures UITestSeeder.seedIfNeeded() (which runs in AppDelegate
        // didFinishLaunchingWithOptions, after PodcastHandle.init()) has
        // already written the fresh seed before the kernel opens the store.
        Self.configurePodcastDataDir(for: podcastHandle)
        nmp_app_start(raw, visibleLimit, emitHz)
    }

    func configure(visibleLimit: UInt32, emitHz: UInt32) {
        nmp_app_configure(raw, visibleLimit, emitHz)
    }

    func stop() {
        nmp_app_stop(raw)
    }

    func reset() {
        nmp_app_reset(raw)
    }

    // ── T118 / G3 — iOS scenePhase → kernel lifecycle bridge ──────────────

    func lifecycleForeground() {
        nmp_app_lifecycle_foreground(raw)
    }

    func lifecycleBackground() {
        nmp_app_lifecycle_background(raw)
    }

    // ── Generic dispatch ──────────────────────────────────────────────────

    /// Dispatch a namespace-keyed action. Returns the synchronous dispatch
    /// result. D6: returns .failure for a null podcast handle.
    ///
    /// ADR-0064: calls `nmp_app_podcast_dispatch_action` (typed byte doorway)
    /// instead of the nmp-ffi ≤ v0.7.2 `nmp_app_dispatch_action` JSON doorway
    /// which was deleted in NMP v0.8.0.
    @discardableResult
    func dispatchAction(namespace: String, body: [String: Any]) -> DispatchResult {
        // Perf: dispatch is a synchronous FFI round-trip on the caller thread
        // (usually main). Time the whole serialize → FFI → parse path so a slow
        // action shows up as a main-thread cost in the Performance view.
        let dispatchStart = DispatchTime.now().uptimeNanoseconds
        defer {
            PerfMetrics.shared.record(
                .dispatchAction,
                micros: Int((DispatchTime.now().uptimeNanoseconds &- dispatchStart) / 1_000))
        }
        guard let handle = podcastHandle else {
            return .failure("podcast handle not initialized")
        }
        guard let data = try? JSONSerialization.data(withJSONObject: body),
              let jsonStr = String(data: data, encoding: .utf8)
        else {
            return .failure("failed to serialize action body")
        }
        let envelope: String? = jsonStr.withCString { jsonPtr in
            namespace.withCString { nsPtr in
                guard let ptr = nmp_app_podcast_dispatch_action(handle, nsPtr, jsonPtr) else {
                    return nil
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }
        guard let envelope else {
            return .failure("dispatch returned a null envelope")
        }
        return DispatchResult.parse(envelope: envelope)
    }

    fileprivate static func decode(pointer: UnsafePointer<CChar>) -> KernelUpdateResult? {
        let start = ContinuousClock.now
        let payload = String(cString: pointer)
        let data = Data(payload.utf8)
        let decoder = KernelDecoding.makeDecoder()
        do {
            let envelope = try decoder.decode(SnapshotEnvelope.self, from: data)
            guard envelope.t == "snapshot" else {
                kbLog.error("unknown envelope tag=\(envelope.t) bytes=\(data.count)")
                return nil
            }
            // NMP v0.5.0 per-domain push path: extract whichever
            // `podcast.*` domain sidecars are present in this frame.
            // `nmp_app_podcast_decode_update_frame` injects typed sidecars
            // under `v.projections[schema_id]`; absent domains carry no sidecar
            // (delta suppression) and MUST NOT overwrite prior state.
            let domainFrames = PodcastDomainFrames.decode(from: data) ?? PodcastDomainFrames()
            let nostrSearchSessions = NostrSearchProjection.decodeSessions(from: data)
            guard domainFrames.hasAnyDomain || !nostrSearchSessions.isEmpty else {
                kbLog.error(
                    "snapshot frame missing all podcast.* domain sidecars bytes=\(data.count)")
                return nil
            }
            // Identity is sourced from the identity domain sidecar when present,
            // otherwise derived from the playback domain (active_account may also
            // appear there on older kernels). Fails open to .empty.
            let identity = KernelIdentityProjection.from(domainFrames: domainFrames)
            // Mandatory NMP v0.1.0 surface (V-67): `store_open_failure` rides the
            // generic snapshot (sibling of `projections`). Read raw, mirroring the
            // identity decode — typed domain envelopes don't model this key.
            let storeOpenFailure = KernelUpdateResult.extractStoreOpenFailure(envelopePayload: data)
            let duration = start.duration(to: .now)
            kbLog.info(
                "decoded ok domains=\(domainFrames.presentDomainNames())")
            return KernelUpdateResult(
                domainFrames: domainFrames,
                identity: identity,
                storeOpenFailure: storeOpenFailure,
                nostrSearchSessions: nostrSearchSessions,
                payloadBytes: data.count,
                callbackReceivedAt: start,
                decodeMicros: duration.microseconds)
        } catch {
            kbLog.error(
                "envelope decode error: \(error.localizedDescription) bytes=\(data.count)")
            return nil
        }
    }
}

// ─── Snapshot envelope ────────────────────────────────────────────────────

private struct SnapshotEnvelope: Decodable {
    let t: String
}

// ─── C callback objects ───────────────────────────────────────────────────

private final class KernelUpdateSink {
    let handler: (KernelUpdateResult) -> Void
    let onPanic: () -> Void
    let signedEvents: SignedEventsRegistry
    let actionResults: ActionResultsRegistry

    init(
        handler: @escaping (KernelUpdateResult) -> Void,
        onPanic: @escaping () -> Void,
        signedEvents: SignedEventsRegistry,
        actionResults: ActionResultsRegistry
    ) {
        self.handler = handler
        self.onPanic = onPanic
        self.signedEvents = signedEvents
        self.actionResults = actionResults
    }
}

private let nmpUpdateCallback: NmpUpdateCallback = { context, bytes, len in
    guard let context, let bytes, len > 0 else { return }
    // Perf: time the whole background frame-processing cost (FlatBuffer→JSON +
    // envelope parse) and record the frame size. This runs on the Rust actor
    // thread, NOT main — background FFI cost, distinct from the main-thread
    // `apply`/`projection` segments. One monotonic clock read is cheap even when
    // metrics are off; the matching `record` is a no-op when disabled.
    let frameStart = DispatchTime.now().uptimeNanoseconds
    // The kernel's update transport is binary FlatBuffers (NMP commit "Replace
    // update transport with FlatBuffers"). Decode the `(bytes, len)` frame to the
    // JSON envelope the shell consumes; `nmp_free_string` reclaims it.
    guard let jsonPtr = nmp_app_podcast_decode_update_frame(bytes, len) else { return }
    defer { nmp_free_string(jsonPtr) }
    let payload = String(cString: jsonPtr)
    let sink = Unmanaged<KernelUpdateSink>.fromOpaque(context).takeUnretainedValue()
    // Drain the `signed_events` and `action_results` projections FIRST — before
    // the panic short-circuit and before the podcast-decode guard below — so a
    // frame that carries a result but no `podcast.snapshot` (or even a panic
    // frame that still flushed a pending sign) never silently drops the result.
    // Both are drain-once frames the kernel clears on emit; the registries
    // retain results so a not-yet-registered continuation still resolves.
    let payloadData = Data(payload.utf8)
    sink.signedEvents.ingest(envelopePayload: payloadData)
    sink.actionResults.ingest(envelopePayload: payloadData)
    if payload.contains("\"t\":\"panic\"") {
        kbLog.fault("NMP_ACTOR_PANIC detected bytes=\(len)")
        sink.onPanic()
        return
    }
    guard let result = PodcastHandle.decode(pointer: jsonPtr) else { return }
    PerfMetrics.shared.record(
        .pushFrameDecode,
        micros: Int((DispatchTime.now().uptimeNanoseconds &- frameStart) / 1_000),
        bytes: Int(len))
    sink.handler(result)
}
