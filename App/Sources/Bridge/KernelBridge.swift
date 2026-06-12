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

    /// Deadline (seconds) for a sign-for-return round-trip. Generous — a remote
    /// (NIP-46 bunker) signer may need a human tap — but bounded so a kernel that
    /// never resolves the id can't hang an upload indefinitely.
    private static let signForReturnTimeout: Double = 60

    /// Buffered resolver for `nmp_app_sign_event_for_return` round-trips. The
    /// `signed_events` projection that carries each result is drain-once: the
    /// kernel clears it on the first emit tick that carries it. Because the
    /// correlation id is only known AFTER the synchronous FFI return, a slow
    /// `await` could miss that single frame. This registry closes the race by
    /// retaining every drained result keyed by id, so `signEventForReturn`
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
        nmp_app_start(raw, 0, visibleLimit, emitHz)
    }

    func configure(visibleLimit: UInt32, emitHz: UInt32) {
        nmp_app_configure(raw, 0, visibleLimit, emitHz)
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
    /// result. D6: never returns a null envelope for a non-null app.
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
        guard let data = try? JSONSerialization.data(withJSONObject: body),
              let jsonStr = String(data: data, encoding: .utf8)
        else {
            return .failure("failed to serialize action body")
        }
        let envelope: String? = jsonStr.withCString { jsonPtr in
            namespace.withCString { nsPtr in
                guard let ptr = nmp_app_dispatch_action(raw, nsPtr, jsonPtr) else {
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

    // ── Sign-for-return (D13 kernel signing seam) ─────────────────────────

    /// Sign an unsigned NIP-01 event draft through the kernel and await the
    /// resulting wire event. NO private key ever crosses into Swift (D13): the
    /// kernel holds the key, signs on the actor thread, and surfaces the result
    /// in the drain-once `signed_events` projection keyed by the returned
    /// correlation id.
    ///
    /// `accountPubkeyHex` selects the signer (empty string → the active
    /// account). `unsignedJSON` is the `{ "kind", "content", "tags",
    /// "created_at"? }` draft shape `nmp_app_sign_event_for_return` accepts —
    /// `created_at` is advisory (the kernel re-stamps it, D7).
    ///
    /// Race-free by construction: the continuation is registered against the id
    /// the synchronous FFI call returns, and `signedEventsRegistry` retains any
    /// result that already drained before the registration completes.
    func signEventForReturn(accountPubkeyHex: String, unsignedJSON: String) async throws -> String {
        let correlationID: String? = accountPubkeyHex.withCString { pkPtr in
            unsignedJSON.withCString { jsonPtr in
                guard let ptr = nmp_app_sign_event_for_return(raw, pkPtr, jsonPtr) else {
                    return nil
                }
                defer { nmp_free_string(ptr) }
                let id = String(cString: ptr)
                return id.isEmpty ? nil : id
            }
        }
        guard let correlationID else {
            throw NostrSignerError.invalidEventForSigning
        }
        // Caller-owned timeout (NMP contract: a null/unstarted app never reaches
        // the kernel, so "the caller's continuation times out"). Race the
        // registry await against a deadline; on expiry, fail + drop the waiter so
        // the upload surfaces a thrown error instead of hanging forever.
        return try await withThrowingTaskGroup(of: String.self) { group in
            group.addTask { [signedEventsRegistry] in
                try await signedEventsRegistry.awaitResult(correlationID: correlationID)
            }
            group.addTask { [signedEventsRegistry] in
                try? await Task.sleep(for: .seconds(Self.signForReturnTimeout))
                signedEventsRegistry.cancel(
                    correlationID: correlationID, with: NostrSignerError.timedOut)
                // Park: the cancel above resumes the real waiter (success or
                // timeout); this task just keeps the group alive until then.
                try await Task.sleep(for: .seconds(3600))
                throw NostrSignerError.timedOut
            }
            defer { group.cancelAll() }
            guard let result = try await group.next() else {
                throw NostrSignerError.timedOut
            }
            return result
        }
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
            guard let domainFrames = PodcastDomainFrames.decode(from: data) else {
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

// ─── Signed-events registry (sign-for-return resolver) ────────────────────

/// Thread-safe find-or-register resolver for `signed_events` projection
/// results. Every kernel frame's `projections["signed_events"]` map is
/// `ingest`ed here under a lock; `awaitResult(correlationID:)` either consumes
/// an already-buffered result or installs a continuation the next `ingest`
/// resolves. This is the structural guarantee that the drain-once frame is
/// never missed between the synchronous `nmp_app_sign_event_for_return` return
/// and the caller's `await`.
final class SignedEventsRegistry: @unchecked Sendable {
    private let lock = NSLock()
    /// Results that drained before a waiter registered. Keyed by correlation id.
    private var buffered: [String: Result<String, Error>] = [:]
    /// Waiters that registered before their result drained.
    private var waiters: [String: CheckedContinuation<String, Error>] = [:]

    /// Ingest one frame's `signed_events` projection. Each value is
    /// `{ "ok": true, "signed_json": "…" }` or `{ "ok": false, "error": "…" }`.
    /// Resolves any registered waiter immediately; otherwise buffers the result.
    func ingest(envelopePayload data: Data) {
        guard
            let raw = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let value = raw["v"] as? [String: Any],
            let projections = value["projections"] as? [String: Any],
            let signed = projections["signed_events"] as? [String: Any],
            !signed.isEmpty
        else { return }

        var resolved: [(CheckedContinuation<String, Error>, Result<String, Error>)] = []
        lock.lock()
        for (correlationID, entry) in signed {
            guard let object = entry as? [String: Any] else { continue }
            let result: Result<String, Error>
            if let ok = object["ok"] as? Bool, ok, let signedJSON = object["signed_json"] as? String {
                result = .success(signedJSON)
            } else {
                let message = (object["error"] as? String) ?? "kernel signing failed"
                result = .failure(NostrSignerError.remoteRejected(message))
            }
            if let waiter = waiters.removeValue(forKey: correlationID) {
                resolved.append((waiter, result))
            } else {
                buffered[correlationID] = result
            }
        }
        lock.unlock()
        // Resume continuations outside the lock.
        for (waiter, result) in resolved { waiter.resume(with: result) }
    }

    /// Await the signed-event JSON for `correlationID`. Returns the flat NIP-01
    /// event JSON on success; throws on a kernel-reported error.
    func awaitResult(correlationID: String) async throws -> String {
        try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            if let buffered = buffered.removeValue(forKey: correlationID) {
                lock.unlock()
                continuation.resume(with: buffered)
                return
            }
            waiters[correlationID] = continuation
            lock.unlock()
        }
    }

    /// Fail an outstanding waiter for `correlationID` with `error` and stop
    /// retaining it. No-op if the result already drained (the waiter is gone).
    /// Used by the caller-owned timeout so a kernel that never resolves the id
    /// (e.g. a null/unstarted app — the NMP contract says "the caller's
    /// continuation times out") surfaces as a thrown error, not a permanent
    /// hang. Also drops any buffered-but-unclaimed result for the id so it
    /// cannot leak.
    func cancel(correlationID: String, with error: Error) {
        lock.lock()
        let waiter = waiters.removeValue(forKey: correlationID)
        buffered.removeValue(forKey: correlationID)
        lock.unlock()
        waiter?.resume(throwing: error)
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

// ─── Swift-side timing wrapper ────────────────────────────────────────────

struct KernelUpdateResult {
    /// Per-domain push-frame sidecars decoded from this tick. Only domains
    /// that actually changed since the last emit are present (delta
    /// suppression). Absent domains MUST NOT overwrite prior composite state.
    let domainFrames: PodcastDomainFrames
    /// Identity slice of the kernel snapshot — `active_account` /
    /// `accounts` / `bunker_handshake` per
    /// `KernelIdentityProjection`.
    let identity: KernelIdentityProjection
    /// Top-level `store_open_failure` diagnostic (V-67). `nil` in healthy
    /// sessions; `Some(reason)` when the kernel could not open its on-disk
    /// LMDB store and fell back to in-memory (this session's data will not
    /// persist). The host MUST surface this to the user.
    let storeOpenFailure: String?
    let payloadBytes: Int
    let callbackReceivedAt: ContinuousClock.Instant
    let decodeMicros: Int
}

extension KernelUpdateResult {
    /// Extract the top-level `store_open_failure` string from a kernel snapshot
    /// wire envelope (`{"t":"snapshot","v":{...}}`). Mirrors the raw second-pass
    /// read in `KernelIdentityProjection.decode` — the typed `PodcastUpdate`
    /// decode intentionally drops this generic-snapshot key. Returns `nil` when
    /// the key is absent (healthy session) or the payload is unparseable.
    static func extractStoreOpenFailure(envelopePayload data: Data) -> String? {
        guard let raw = try? JSONSerialization.jsonObject(with: data),
              let outer = raw as? [String: Any],
              let value = outer["v"] as? [String: Any]
        else { return nil }
        return value["store_open_failure"] as? String
    }
}

// ─── Duration microseconds helper ────────────────────────────────────────

extension Duration {
    var microseconds: Int {
        let parts = components
        return Int(parts.seconds) * 1_000_000 + Int(parts.attoseconds / 1_000_000_000_000)
    }
}
