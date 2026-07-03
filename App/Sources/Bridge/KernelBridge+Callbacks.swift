import Darwin
import Foundation
import os.log

final class KernelAgentAskSink: PodcastAgentAskSink, @unchecked Sendable {
    weak var handle: PodcastHandle?

    init(handle: PodcastHandle) {
        self.handle = handle
    }

    func onAgentAskEvent(eventJson: String) {
        guard let handle,
              let data = eventJson.data(using: .utf8),
              let response = try? KernelDecoding.makeDecoder().decode(
                KernelModel.AgentAskResponse.self, from: data)
        else { return }
        Task { @MainActor in
            handle.onAgentAskEvent?(response)
        }
    }
}

// ─── Podcast projection registration ─────────────────────────────────────
//
// `PodcastApp` wires the podcast-specific projection into the kernel. Called
// once from `PodcastHandle.init()` before `start()`.

extension PodcastHandle {
    /// Register the capability callback then the podcast snapshot projection.
    /// Must be called once after `PodcastApp` construction and before `start()`.
    func registerPodcastProjection() {
        let bridge = SyncCapabilityBridge()
        syncBridge = bridge
        podcastApp.setCapabilityCallback(sink: bridge)
        let askSink = KernelAgentAskSink(handle: self)
        agentAskSink = askSink
        podcastApp.setAgentAskSink(sink: askSink)

        let handleBits = podcastApp.podcastHandle()
        podcastHandle = UnsafeMutableRawPointer(bitPattern: UInt(handleBits))
        if podcastHandle == nil {
            kbLog.error("PodcastApp.podcastHandle returned 0 — projection unwired")
            return
        }
        // NOTE: configurePodcastDataDir is intentionally NOT called here.
        // `nmp_app_podcast_set_data_dir` reads podcasts.json immediately at
        // call time (data_dir.rs). Because `PodcastHandle.init()` runs during
        // `PodcastrApp` struct initialisation — before SwiftUI evaluates body
        // and before `AppDelegate.didFinishLaunchingWithOptions` fires —
        // calling it here races with `UITestSeeder.seedIfNeeded()`.  The seeder
        // must overwrite podcasts.json *before* the kernel reads it, so
        // `configurePodcastDataDir` is deferred to `start()`, which is called
        // from `KernelModel.start()` → `.task {}`, well after AppDelegate and
        // UITestSeeder have completed.
    }

    static func configurePodcastDataDir(for app: PodcastApp) {
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
            app.setPodcastDataDir(path: directory.path)
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
    /// feed fetch. The async `HttpCapability` is the canonical instance shared
    /// by `PodcastCapabilities` and `SyncCapabilityBridge`; it fires the report
    /// sink from its `URLSession` completion — a private **background** queue,
    /// never main — so the FFI call below runs off both the main and the actor
    /// threads. Unlike audio/download there is **no**
    /// `MainActor.assumeIsolated` hop (that would crash off-main):
    /// `nmp_app_podcast_http_report` only touches the shared store + signal and
    /// wakes the projection push seam, returning NULL (no follow-up command).
    /// The `podcastHandle` is captured by value here, after
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
                // Voice state changes ride the `podcast.voice` push sidecar.
                // A full snapshot pull is no longer needed here; the push frame
                // produced by the domain-scoped bump in voice_handler::mutate_voice_state
                // will surface voice state on the next emit.
                if let result = nmp_app_podcast_voice_report(handle, reportJSON) {
                    // Reserved: when Rust starts returning a follow-up
                    // `VoiceCommand`, decode + execute it here. For the
                    // capability scaffold the symbol always returns NULL.
                    nmp_free_string(result)
                }
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
        podcastApp.setAgentAskSink(sink: nil)
        agentAskSink = nil
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
        podcastApp.podcastSnapshotRev()
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
        guard let json = podcastApp.podcastSnapshot() else {
            return PodcastUpdate()
        }
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
