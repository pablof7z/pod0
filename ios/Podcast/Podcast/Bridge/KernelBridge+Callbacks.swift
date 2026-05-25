import Darwin
import Foundation
import os.log

// ─── Capability callback ──────────────────────────────────────────────────
//
// One C callback handles all kernel-issued capability requests. Runs on the
// Rust actor thread (a background thread), so the bridge it calls MUST be
// thread-safe and synchronous.
//
// `SyncCapabilityBridge` is retained via `bridgeBox` on `PodcastHandle` so the
// registered `context` pointer stays valid until `nmp_app_set_capability_callback`
// is called with `nil` or the handle is deallocated.

private let podcastCapabilityCallback: NmpCapabilityCallback = { context, requestJSON in
    guard let context, let requestJSON else {
        // D6 — null args: return a malloc-allocated error envelope.
        let err = strdup("{\"namespace\":\"\",\"correlation_id\":\"\",\"result_json\":\"{\\\"status\\\":\\\"error\\\",\\\"message\\\":\\\"null-args\\\"}\"}")
        return err
    }
    let bridge = Unmanaged<SyncCapabilityBridge>.fromOpaque(context).takeUnretainedValue()
    let requestStr = String(cString: requestJSON)
    let response = bridge.handle(requestJSON: requestStr)
    // MUST use strdup: Rust takes ownership via CString::from_raw, which
    // requires a malloc-compatible allocation.
    return strdup(response)
}

// ─── Podcast projection registration ─────────────────────────────────────
//
// `nmp_app_podcast_register` wires the podcast-specific projection into the
// kernel. Called once from `PodcastHandle.init()` before `start()`. The handle
// is dropped in `deinit` via `unregisterPodcastProjectionIfNeeded()`.

extension PodcastHandle {
    /// Register the capability callback then the podcast snapshot projection.
    /// Must be called once after `nmp_app_new()` and before `start()`.
    func registerPodcastProjection() {
        let bridge = SyncCapabilityBridge()
        // Keep a strong reference on self so the context pointer remains valid
        // for the lifetime of the registered callback.
        syncBridge = bridge
        let ctx = Unmanaged.passUnretained(bridge).toOpaque()
        nmp_app_set_capability_callback(raw, ctx, podcastCapabilityCallback)

        podcastHandle = nmp_app_podcast_register(raw)
        if podcastHandle == nil {
            kbLog.error("nmp_app_podcast_register returned NULL — projection unwired")
        }
    }

    /// Wire the async iOS→Rust audio report channel.
    ///
    /// Must be called from a `@MainActor` context (e.g. `KernelModel.init()`)
    /// after `registerPodcastProjection()` has run. `AudioCapability` fires
    /// the `sendReport` closure from its own `@MainActor` methods; the closure
    /// uses `MainActor.assumeIsolated` to safely reach back into
    /// `PodcastCapabilities.shared` from the non-isolated closure type.
    @MainActor
    func attachAudioReportChannel() {
        PodcastCapabilities.shared.audio.attach { [weak self] reportJSON in
            MainActor.assumeIsolated {
                guard let handle = self?.podcastHandle else { return }
                guard let result = nmp_app_podcast_audio_report(handle, reportJSON)
                else { return }
                defer { nmp_app_free_string(result) }
                let followUpJSON = String(cString: result)
                guard
                    let data = followUpJSON.data(using: .utf8),
                    let command = try? JSONDecoder().decode(AudioCommand.self, from: data)
                else { return }
                PodcastCapabilities.shared.audio.execute(command)
            }
        }
    }

    func unregisterPodcastProjectionIfNeeded() {
        guard let handle = podcastHandle else { return }
        nmp_app_podcast_unregister(handle)
        podcastHandle = nil
    }

    func reregisterPodcastProjection() {
        unregisterPodcastProjectionIfNeeded()
        registerPodcastProjection()
    }

    /// Pull the latest snapshot from the podcast projection. Returns `.empty`
    /// when the handle is not registered or the projection serialization fails
    /// (D6 — never crashes, degrades to placeholder).
    func podcastSnapshot() -> PodcastUpdate {
        guard let handle = podcastHandle,
              let ptr = nmp_app_podcast_snapshot(handle)
        else {
            return PodcastUpdate()
        }
        defer { nmp_app_podcast_snapshot_free(ptr) }
        let json = String(cString: ptr)
        guard let data = json.data(using: .utf8) else { return PodcastUpdate() }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return (try? decoder.decode(PodcastUpdate.self, from: data)) ?? PodcastUpdate()
    }
}
