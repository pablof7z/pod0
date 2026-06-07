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
        // On a fresh seed (NOT relaunch), wipe the Swift persistence store so
        // stale playback positions from a prior test don't bleed into this run.
        // The AppStateStore recovery path in applyKernelState reads priorEpisodesByID
        // which is populated from persistence.load() — if the SQLite still has a
        // non-zero position from the last test, that position gets recovered and the
        // episode shows "Resume" instead of "Play", breaking P0-03 / P0-04 clean-state
        // tests. Wiping here ensures every non-relaunch launch starts at position 0.
        if !CommandLine.arguments.contains("--UITestSeedRelaunch") {
            Persistence.shared.reset()
            // reset() deletes the SQLite file, but file deletion silently fails
            // when SQLite keeps the file open via its own connection. Use a SQL
            // DELETE instead — works even on an open file, guaranteed to clear
            // all rows so hydrateEpisodesPreservingMetadata finds nothing and
            // returns an empty episode list rather than restoring a stale position.
            try? Persistence.shared.episodeStore.replaceAll([])
        }
        // Prefer the locally downloaded MP3 over the network URL so AVPlayer
        // plays from disk (reliable in the simulator) rather than streaming
        // from the NPR CDN (which the simulator's sandboxed network may block).
        // The file lands here when the episode is downloaded during an earlier
        // test run; its presence is stable across runs within the same container.
        // Copy the bundled test MP3 into the EpisodeDownloadStore directory so
        // AudioEngine.load() can resolve the file without any network access.
        // EpisodeDownloadStore stores files at:
        //   <AppSupport>/podcastr/downloads/<EPISODE-UUID>.<ext>
        // The episode UUID and .mp3 extension are fixed by the seed below.
        // UITestSeeder runs inside the current process (new data container), so
        // the copy lands in the live container and survives a force-quit, giving
        // the resume-persistence test a durable local file to play.
        let episodeUUID = "A1A1FFFF-0001-0002-0001-000000000001"
        let downloadDir = base
            .appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("downloads", isDirectory: true)
        try? FileManager.default.createDirectory(
            at: downloadDir, withIntermediateDirectories: true)
        let destMP3 = downloadDir.appendingPathComponent("\(episodeUUID).mp3")
        if let bundledMP3 = Bundle.main.url(forResource: "test-episode", withExtension: "mp3"),
           !FileManager.default.fileExists(atPath: destMP3.path) {
            try? FileManager.default.copyItem(at: bundledMP3, to: destMP3)
        }
        let enclosureURL = "https://npr.simplecastaudio.com/d3081dd9-fcaf-445a-977c-4f56c28f5a6e/episodes/e55b1946-2658-4592-9afe-1c2a3033a31c/audio/128/default.mp3"
        let downloadState = "{\"state\": \"not_downloaded\"}"

        // Read any position the previous session wrote to the App Group SQLite
        // ONLY when --UITestSeedRelaunch is present — that flag is set by tests
        // that need position to survive a force-quit + cold relaunch (P0-04b).
        // Without the flag every test starts at position 0 so that a prior test's
        // persisted state doesn't bleed into the next one's Play/Resume assertion.
        //
        // Read directly from EpisodeSQLiteStore rather than through
        // Persistence.load() so that the position is found even when the JSON
        // metadata file doesn't yet exist (e.g. on the very first test run in a
        // fresh simulator). Persistence.load() requires the JSON to exist before
        // it tries SQLite; the SQLite is the durable record that survives force-quit.
        let targetEpisodeID = UUID(uuidString: episodeUUID)!
        var persistedPosition: Double = 0.0
        if CommandLine.arguments.contains("--UITestSeedRelaunch") {
            let sqliteEpisodes = (try? Persistence.shared.episodeStore.loadAll()) ?? []
            if let ep = sqliteEpisodes.first(where: { $0.id == targetEpisodeID }),
               ep.playbackPosition > 0 {
                persistedPosition = ep.playbackPosition
            }
        }

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
              "enclosure_url": "\(enclosureURL)",
              "enclosure_mime_type": "audio/mpeg",
              "position_secs": \(persistedPosition),
              "played": false,
              "is_starred": false,
              "download_state": \(downloadState),
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
