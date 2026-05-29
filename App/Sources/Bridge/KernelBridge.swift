import Darwin
import Foundation
import os.log

let kbLog = Logger(subsystem: "io.f7z.podcast", category: "KernelBridge")

/// Mirror of `KERNEL_SCHEMA_VERSION` (Rust: `crates/nmp-core/src/update_envelope.rs`).
/// Must be bumped in lock-step when the Rust constant changes.
private let KERNEL_SCHEMA_VERSION: UInt32 = 1

/// Thin C-FFI wrapper around the `nmp_app_podcast` static library.
final class PodcastHandle {
    let raw: UnsafeMutableRawPointer
    private var updateSink: KernelUpdateSink?
    /// Opaque handle returned by `nmp_app_podcast_register`.
    var podcastHandle: UnsafeMutableRawPointer?
    /// Retained bridge passed as `context` to `nmp_app_set_capability_callback`.
    var syncBridge: SyncCapabilityBridge?

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
        let sink = KernelUpdateSink(handler: handler, onPanic: onPanic)
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
                defer { nmp_app_free_string(ptr) }
                return String(cString: ptr)
            }
        }
        guard let envelope else {
            return .failure("dispatch returned a null envelope")
        }
        return DispatchResult.parse(envelope: envelope)
    }

    /// Decode the `PodcastUpdate` from the push frame's
    /// `projections["podcast.snapshot"]` slice (registered via the canonical
    /// snapshot-projection seam). Returns `nil` when the projection is absent or
    /// malformed. Mirrors `KernelIdentityProjection.decode`'s raw-read approach.
    fileprivate static func decodePodcastUpdate(envelopePayload data: Data) -> PodcastUpdate? {
        guard
            let raw = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let value = raw["v"] as? [String: Any],
            let projections = value["projections"] as? [String: Any],
            let podcast = projections["podcast.snapshot"],
            let podcastData = try? JSONSerialization.data(withJSONObject: podcast)
        else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        do {
            return try decoder.decode(PodcastUpdate.self, from: podcastData)
        } catch {
            kbLog.error("podcast.snapshot decode FAILED: \(error) bytes=\(podcastData.count)")
            return nil
        }
    }

    fileprivate static func decode(pointer: UnsafePointer<CChar>) -> KernelUpdateResult? {
        let start = ContinuousClock.now
        let payload = String(cString: pointer)
        let data = Data(payload.utf8)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        do {
            let envelope = try decoder.decode(SnapshotEnvelope.self, from: data)
            guard envelope.t == "snapshot" else {
                kbLog.error("unknown envelope tag=\(envelope.t) bytes=\(data.count)")
                return nil
            }
            // The podcast projection rides the generic push frame under
            // `projections["podcast.snapshot"]` (registered via the canonical
            // `register_snapshot_projection` seam). Decode the `PodcastUpdate`
            // out of it — the envelope's `v` is the generic kernel snapshot, not
            // the podcast shape.
            guard let update = Self.decodePodcastUpdate(envelopePayload: data) else {
                kbLog.error("snapshot frame missing podcast.snapshot projection bytes=\(data.count)")
                return nil
            }
            // Identity projection slice (`projections.active_account` / `accounts`
            // / `bunker_handshake`) from the same raw envelope.
            let identity = KernelIdentityProjection.decode(envelopePayload: data)
            // Mandatory NMP v0.1.0 surface (V-67): the kernel sets the
            // top-level `store_open_failure` string when the configured LMDB
            // store failed to open and it fell back to in-memory. It rides the
            // generic snapshot (sibling of `projections`), which `PodcastUpdate`
            // does not model — so read it raw, mirroring the identity decode.
            let storeOpenFailure = KernelUpdateResult.extractStoreOpenFailure(envelopePayload: data)
            let duration = start.duration(to: .now)
            kbLog.info("decoded ok rev=\(update.rev)")
            return KernelUpdateResult(
                update: update,
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

    init(handler: @escaping (KernelUpdateResult) -> Void, onPanic: @escaping () -> Void) {
        self.handler = handler
        self.onPanic = onPanic
    }
}

private let nmpUpdateCallback: NmpUpdateCallback = { context, bytes, len in
    guard let context, let bytes, len > 0 else { return }
    // The kernel's update transport is binary FlatBuffers (NMP commit "Replace
    // update transport with FlatBuffers"). Decode the `(bytes, len)` frame to the
    // JSON envelope the shell consumes; `nmp_app_free_string` reclaims it.
    guard let jsonPtr = nmp_app_podcast_decode_update_frame(bytes, len) else { return }
    defer { nmp_app_free_string(jsonPtr) }
    let payload = String(cString: jsonPtr)
    let sink = Unmanaged<KernelUpdateSink>.fromOpaque(context).takeUnretainedValue()
    if payload.contains("\"t\":\"panic\"") {
        kbLog.fault("NMP_ACTOR_PANIC detected bytes=\(len)")
        sink.onPanic()
        return
    }
    guard let result = PodcastHandle.decode(pointer: jsonPtr) else { return }
    sink.handler(result)
}

// ─── Swift-side timing wrapper ────────────────────────────────────────────

struct KernelUpdateResult {
    let update: PodcastUpdate
    /// Identity slice of the kernel snapshot — `active_account` /
    /// `accounts` / `bunker_handshake` per
    /// `KernelIdentityProjection.decode`.
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
