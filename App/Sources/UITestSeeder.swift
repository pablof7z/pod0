import Foundation

/// Writes a minimal This American Life library seed to the kernel's
/// `podcasts.json` when the app is launched with `--UITestSeed`.
///
/// Call from `AppMain.body` before `kernelModel.start()`. The write is
/// synchronous and idempotent (skipped if the file already has real content).
/// The kernel reads `podcasts.json` at `nmp_app_start` time, so the seed
/// must land before that call.
///
/// Never compiled out — the `CommandLine.arguments` guard is the safety valve
/// so this is a no-op in production. Kept in the main target (not the test
/// target) because it must run inside the app process where it has access to
/// `applicationSupportDirectory`.
enum UITestSeeder {
    static func seedIfNeeded() {
        guard CommandLine.arguments.contains("--UITestSeed") else { return }
        // Request synchronous position-flush writes so that the SQLite episode
        // store is updated before a SIGKILL force-quit can race the background
        // Task. Only the flushPendingPositions path uses this; all other writes
        // keep their normal background-Task behavior so the app stays responsive.
        AppStateStore.synchronousPositionFlushForUITests = true
        guard let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask).first
        else { return }
        let dir = base.appendingPathComponent("PodcastLibrary", isDirectory: true)
        let file = dir.appendingPathComponent("podcasts.json")
        // Always overwrite when running under --UITestSeed: the kernel may have
        // replaced a prior seed with real RSS data (or a stale seed from the last
        // test run). Re-seeding gives every test run a known-good starting state.
        try? FileManager.default.removeItem(at: file)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let seed = """
        {
          "schema_version": 1,
          "podcasts": [{
            "podcast": {
              "id": "a1a1ffff-0001-0001-0001-000000000001",
              "feed_url": "https://test.podcast.local/rss.xml",
              "title": "This American Life",
              "author": "This American Life",
              "image_url": "https://thisamericanlife.org/sites/all/themes/thislife/img/tal-logo-3000x3000.png",
              "description": "Weekly public radio.",
              "categories": [],
              "discovered_at": "2026-06-06T13:00:00Z",
              "nostr_visibility": "private",
              "title_is_placeholder": false
            },
            "episodes": [{
              "id": "a1a1ffff-0001-0002-0001-000000000001",
              "podcast_id": "a1a1ffff-0001-0001-0001-000000000001",
              "guid": "37536 at https://www.thisamericanlife.org",
              "title": "137: The Book That Changed Your Life",
              "description": "Books that shaped peoples lives.",
              "pub_date": "2026-05-01T00:00:00Z",
              "duration_secs": 300.0,
              "enclosure_url": "https://npr.simplecastaudio.com/d3081dd9-fcaf-445a-977c-4f56c28f5a6e/episodes/e55b1946-2658-4592-9afe-1c2a3033a31c/audio/128/default.mp3",
              "enclosure_mime_type": "audio/mpeg",
              "position_secs": 0.0,
              "played": false,
              "is_starred": false,
              "download_state": {"state": "not_downloaded"},
              "transcript_state": {"state": "none"},
              "triage_is_hero": false,
              "metadata_indexed": false
            }],
            "auto_download": false,
            "cellular_allowed": false
          }],
          "has_completed_onboarding": true,
          "memory_facts": [],
          "ad_segments": [],
          "episode_triage": [],
          "metadata_indexed_episodes": [],
          "transcript_status_overrides": [],
          "settings": {},
          "queue": [],
          "pending_wifi_downloads": []
        }
        """
        try? seed.data(using: .utf8)?.write(to: file, options: .atomic)
    }
}
