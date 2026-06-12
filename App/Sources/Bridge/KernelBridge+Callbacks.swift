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
    ///
    /// The FFI call is dispatched to a background serial queue so that when
    /// `maybe_auto_advance` re-enters `SyncCapabilityBridge` and calls
    /// `DispatchQueue.main.sync`, the calling thread is not main and the
    /// hop cannot deadlock.
    @MainActor
    func attachAudioReportChannel() {
        let reportQueue = DispatchQueue(label: "podcast.audio-report", qos: .utility)
        PodcastCapabilities.shared.audio.attach { [weak self] reportJSON in
            MainActor.assumeIsolated {
                // Capture self strongly for the background hop so the handle
                // pointer stays valid until the FFI call completes. The FFI
                // call runs off-main so Rust's `maybe_auto_advance` re-entry
                // (which may call `DispatchQueue.main.sync` through
                // `SyncCapabilityBridge`) cannot deadlock against this thread.
                guard let self else { return }
                reportQueue.async { [self] in
                    guard let handle = self.podcastHandle else { return }
                    guard let result = nmp_app_podcast_audio_report(handle, reportJSON) else {
                        // Error / degrade path — nothing actionable, don't pull.
                        return
                    }
                    defer { nmp_free_string(result) }
                    let responseJSON = String(cString: result)
                    guard let data = responseJSON.data(using: .utf8) else { return }
                    let decoder = KernelDecoding.makeDecoder()
                    guard let response = try? decoder.decode(
                        AudioReportResponse.self, from: data) else {
                        return
                    }
                    Task { @MainActor in
                        // Execute the follow-up command (decoded with a PLAIN
                        // decoder — `AudioCommand` uses coding keys a snake-case
                        // conversion would break).
                        if let followUpJSON = response.followUp,
                           let cmdData = followUpJSON.data(using: .utf8),
                           let command = try? JSONDecoder().decode(
                            AudioCommand.self, from: cmdData) {
                            PodcastCapabilities.shared.audio.execute(command)
                        }
                        // Update the live player surface from the inline state, and
                        // pull the full library only when structural state changed
                        // (`Playing`/`BufferingProgress` ticks ride the inline
                        // `nowPlaying` and never decode the library).
                        self.onAudioReport?(response.nowPlaying, response.durableChanged)
                    }
                }
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
    /// The FFI returns a JSON `DownloadReportResponse`
    /// (`{ follow_up?, downloads?, durable_changed }`), or NULL on error
    /// (D6 degrade — treated as "nothing actionable"). Progress ticks ride the
    /// inline `downloads` field and do NOT bump the global `rev`; only a
    /// `durable_changed` report (completion/cancellation) warrants the full
    /// snapshot pull. See `nmp_app_podcast_download_report`.
    @MainActor
    func attachDownloadReportChannel() {
        PodcastCapabilities.shared.download.attach { [weak self] reportJSON in
            MainActor.assumeIsolated {
                guard let self, let handle = self.podcastHandle else { return }
                guard let result = nmp_app_podcast_download_report(handle, reportJSON) else {
                    // Error / degrade path — nothing actionable, don't pull.
                    return
                }
                defer { nmp_free_string(result) }
                let responseJSON = String(cString: result)
                guard let data = responseJSON.data(using: .utf8) else { return }
                let decoder = KernelDecoding.makeDecoder()
                guard let response = try? decoder.decode(DownloadReportResponse.self, from: data) else {
                    return
                }
                // Execute the follow-up command (decoded with a PLAIN decoder —
                // `DownloadCommand` uses explicit `episode_id` coding keys that a
                // snake-case conversion would break).
                if let followUpJSON = response.followUp,
                   let cmdData = followUpJSON.data(using: .utf8),
                   let command = try? JSONDecoder().decode(DownloadCommand.self, from: cmdData) {
                    PodcastCapabilities.shared.download.execute(command)
                }
                // Update the live download surface from the inline snapshot, and
                // pull the full library only when durable state actually changed.
                self.onDownloadReport?(response.downloads, response.durableChanged)
            }
        }
    }

    /// Wire the async iOS→Rust HTTP report channel for the optimistic-subscribe
    /// feed fetch. The async `HttpCapability` (owned by `SyncCapabilityBridge`,
    /// the live capability-callback router) fires the report sink from its
    /// `URLSession` completion — a private **background** queue, never main — so
    /// the FFI call below runs off both the main and the actor threads. Unlike
    /// audio/download there is **no** `MainActor.assumeIsolated` hop (that would
    /// crash off-main): `nmp_app_podcast_http_report` only touches the shared
    /// store + signal and wakes the projection push seam, returning NULL (no
    /// follow-up command). The `podcastHandle` is captured by value here, after
    /// `registerPodcastProjection()` has set it; it stays valid for the app's
    /// lifetime (the callback is cleared before the handle is freed).
    @MainActor
    func attachHttpReportChannel() {
        guard let handle = podcastHandle else { return }
        syncBridge?.attachHttpReport { reportJSON in
            if let result = nmp_app_podcast_http_report(handle, reportJSON) {
                nmp_free_string(result)
            }
        }
    }

    /// Decoded shape of `nmp_app_podcast_download_report`'s JSON response.
    /// `followUp` is the raw `DownloadCommand` JSON string (decoded separately,
    /// not nested, to preserve its explicit snake_case coding keys).
    private struct DownloadReportResponse: Decodable {
        var followUp: String?
        var downloads: DownloadQueueSnapshot?
        var durableChanged: Bool
    }

    /// Decoded shape of `nmp_app_podcast_audio_report`'s JSON response.
    /// `followUp` is the raw `AudioCommand` JSON string (decoded separately, not
    /// nested, to preserve its coding keys). `nowPlaying` is the same
    /// `PlayerState` shape as `PodcastUpdate.now_playing`.
    private struct AudioReportResponse: Decodable {
        var followUp: String?
        var nowPlaying: PlayerState?
        var durableChanged: Bool
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
                    nmp_free_string(result)
                }
                self.onSnapshotMaybeChanged?()
            }
        }
    }

    /// Start the network monitor and deliver an initial `ConnectivityChanged`
    /// report so Rust's `is_on_wifi` flag is primed before the first feed
    /// refresh. Must be called from a `@MainActor` context after
    /// `registerPodcastProjection()`.
    @MainActor
    func startNetworkMonitor() {
        guard let handle = podcastHandle else { return }
        // When Wi-Fi is restored after a cellular-only period, dispatch the
        // deferred downloads action so episodes that were skipped on cellular
        // are downloaded immediately rather than waiting for the next refresh.
        PodcastCapabilities.shared.network.onWifiRestored = { [weak self] in
            guard let self else { return }
            _ = self.dispatchAction(namespace: "podcast",
                                    body: ["op": "dispatch_deferred_wifi_downloads"])
        }
        PodcastCapabilities.shared.network.start(handle: handle)
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
        // Perf: a full-library serialize + decode on the caller thread (usually
        // main, via the synchronous post-dispatch pull). Time it and record the
        // payload size — this is the snapshot-decode hot path that scales O(N)
        // with library size.
        let pullStart = DispatchTime.now().uptimeNanoseconds
        var pullBytes = 0
        defer {
            PerfMetrics.shared.record(
                .snapshotPull,
                micros: Int((DispatchTime.now().uptimeNanoseconds &- pullStart) / 1_000),
                bytes: pullBytes)
        }
        guard let handle = podcastHandle,
              let ptr = nmp_app_podcast_snapshot(handle)
        else {
            return PodcastUpdate()
        }
        defer { nmp_app_podcast_snapshot_free(ptr) }
        let json = String(cString: ptr)
        guard let data = json.data(using: .utf8) else { return PodcastUpdate() }
        pullBytes = data.count
        do {
            let update = try KernelDecoding.decodePodcastUpdate(from: data)
            guard update.schemaVersion == KERNEL_SCHEMA_VERSION else {
                kbLog.fault(
                    "podcastSnapshot REJECTED: schema_version \(update.schemaVersion) != expected \(KERNEL_SCHEMA_VERSION) — failing closed on kernel/shell schema mismatch")
                return PodcastUpdate()
            }
            return update
        } catch {
            kbLog.error("podcastSnapshot decode: \(error, privacy: .public)")
            return PodcastUpdate()
        }
    }
}
