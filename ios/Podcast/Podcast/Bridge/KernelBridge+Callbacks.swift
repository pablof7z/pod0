import Foundation
import os.log

// ─── Podcast projection registration ─────────────────────────────────────
//
// `nmp_app_podcast_register` wires the podcast-specific projection into the
// kernel. Called once from `PodcastHandle.init()` before `start()`. The handle
// is dropped in `deinit` via `unregisterPodcastProjectionIfNeeded()`.

extension PodcastHandle {
    /// Register the podcast snapshot projection on the kernel. Must be called
    /// once after `nmp_app_new()` and before `start()`.
    func registerPodcastProjection() {
        podcastHandle = nmp_app_podcast_register(raw)
        if podcastHandle == nil {
            kbLog.error("nmp_app_podcast_register returned NULL — projection unwired")
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
