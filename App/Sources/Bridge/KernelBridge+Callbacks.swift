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
            return
        }

        // Wire the podcast library persistence directory. Lives under
        // Application Support so it follows the iOS "user data, not synced
        // via iTunes file sharing" convention. Distinct from the NMP/
        // EventStore directory (`configureStoragePath`) so the two stores
        // can be wiped independently for debugging.
        Self.configurePodcastDataDir(for: podcastHandle)
    }

    private static func configurePodcastDataDir(for handle: UnsafeMutableRawPointer?) {
        guard let handle else { return }
        guard let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask).first
        else {
            kbLog.error("no applicationSupportDirectory — library persistence disabled")
            return
        }
        let directory = base.appendingPathComponent("PodcastLibrary", isDirectory: true)
        do {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true)
            directory.path.withCString { nmp_app_podcast_set_data_dir(handle, $0) }
        } catch {
            kbLog.error(
                "failed to create PodcastLibrary directory: \(error.localizedDescription, privacy: .public)")
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
                guard let self, let handle = self.podcastHandle else { return }
                guard let result = nmp_app_podcast_audio_report(handle, reportJSON) else {
                    // No follow-up command, but the report still bumped `rev`
                    // (e.g. position/now-playing) — pull it through reactively.
                    self.onSnapshotMaybeChanged?()
                    return
                }
                defer { nmp_app_free_string(result) }
                let followUpJSON = String(cString: result)
                if let data = followUpJSON.data(using: .utf8),
                   let command = try? JSONDecoder().decode(AudioCommand.self, from: data) {
                    PodcastCapabilities.shared.audio.execute(command)
                }
                // The report bumped the podcast `rev`; surface it reactively
                // (event-driven, not polled).
                self.onSnapshotMaybeChanged?()
            }
        }
    }

    /// Wire the async iOS→Rust download report channel.
    ///
    /// Mirrors `attachAudioReportChannel()`. Must be called from a
    /// `@MainActor` context (e.g. `KernelModel.init()`) after
    /// `registerPodcastProjection()` has run. `DownloadCapability` fires the
    /// `sendReport` closure from its own `@MainActor` methods; the closure
    /// uses `MainActor.assumeIsolated` to safely reach back into
    /// `PodcastCapabilities.shared` from the non-isolated closure type.
    ///
    /// The FFI return is NULL when no follow-up is needed. When a report
    /// frees a queue slot, Rust may return the next `DownloadCommand` for
    /// the capability to execute immediately.
    @MainActor
    func attachDownloadReportChannel() {
        PodcastCapabilities.shared.download.attach { [weak self] reportJSON in
            MainActor.assumeIsolated {
                guard let self, let handle = self.podcastHandle else { return }
                guard let result = nmp_app_podcast_download_report(handle, reportJSON) else {
                    // No follow-up command, but the report bumped `rev`
                    // (download progress/state) — pull it through reactively.
                    self.onSnapshotMaybeChanged?()
                    return
                }
                defer { nmp_app_free_string(result) }
                let followUpJSON = String(cString: result)
                if let data = followUpJSON.data(using: .utf8),
                   let command = try? JSONDecoder().decode(DownloadCommand.self, from: data) {
                    PodcastCapabilities.shared.download.execute(command)
                }
                // The report bumped the podcast `rev`; surface it reactively.
                self.onSnapshotMaybeChanged?()
            }
        }
    }

    /// Wire the async iOS→Rust voice report channel. Mirrors
    /// `attachAudioReportChannel()`. Voice has no synchronous follow-up
    /// command surface yet — `nmp_app_podcast_voice_report` always
    /// returns NULL — but the call is still routed through Rust so the
    /// `voice_state` projection updates and the next snapshot tick
    /// surfaces the change.
    @MainActor
    func attachVoiceReportChannel() {
        PodcastCapabilities.shared.voice.attach { [weak self] reportJSON in
            MainActor.assumeIsolated {
                guard let self, let handle = self.podcastHandle else { return }
                // The voice report bumps the podcast `rev` (voice_state:
                // listening / transcript / speaking) and returns no push frame,
                // so surface it reactively like the audio/download reports —
                // otherwise voice state is invisible until an unrelated dispatch.
                if let result = nmp_app_podcast_voice_report(handle, reportJSON) {
                    // Reserved: when Rust starts returning a follow-up
                    // `VoiceCommand`, decode + execute it here. For the
                    // capability scaffold the symbol always returns NULL.
                    nmp_app_free_string(result)
                }
                self.onSnapshotMaybeChanged?()
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

    /// Cheap rev check — reads the Rust atomic counter without serializing
    /// the full snapshot. Use this to skip `podcastSnapshot()` when the
    /// rev hasn't advanced since the last decoded snapshot.
    func podcastSnapshotRev() -> UInt64 {
        guard let handle = podcastHandle else { return 0 }
        return nmp_app_podcast_snapshot_rev(handle)
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
