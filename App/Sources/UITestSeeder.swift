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
    static func seededDownloadURL(episodeID: String, sourceURL: URL?) -> URL {
        DownloadCapability.destinationURL(
            for: episodeID,
            sourceURL: sourceURL,
            kind: .episode
        )
    }

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
        // Also wipe the kernel-owned clips sidecar on EVERY --UITestSeed so a
        // prior run's seeded clip (e.g. the orphan clip below) can't contaminate
        // a later test that seeds without --UITestSeedOrphanClip. The orphan
        // branch rewrites clips.json afterwards when that flag IS present.
        try? FileManager.default.removeItem(at: dir.appendingPathComponent("clips.json"))
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
        let episodeUUID = "A1A1FFFF-0001-0002-0001-000000000001"
        let enclosureURL = "https://npr.simplecastaudio.com/d3081dd9-fcaf-445a-977c-4f56c28f5a6e/episodes/e55b1946-2658-4592-9afe-1c2a3033a31c/audio/128/default.mp3"
        let sourceURL = URL(string: enclosureURL)
        // Copy the bundled test MP3 into the same canonical Downloads directory
        // used by DownloadCapability and EpisodeDownloadStore so playback and
        // the Rust download projection both see the local file after restart.
        let destMP3 = seededDownloadURL(episodeID: episodeUUID, sourceURL: sourceURL)
        try? FileManager.default.createDirectory(
            at: destMP3.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        if let bundledMP3 = Bundle.main.url(forResource: "test-episode", withExtension: "mp3"),
           !FileManager.default.fileExists(atPath: destMP3.path) {
            try? FileManager.default.copyItem(at: bundledMP3, to: destMP3)
        }
        let attrs = try? FileManager.default.attributesOfItem(atPath: destMP3.path)
        let localBytes = (attrs?[.size] as? NSNumber)?.int64Value ?? 0
        let localPathLiteral = jsonStringLiteral(destMP3.path)
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

        // ep2 UUID — seeded into the kernel library so the Queue action is accepted
        // and the authoritative projection includes it. Without this entry the
        // kernel silently drops the ep2 enqueue, the snapshot clobbers the
        // optimistic local state, and the "Queued" label reverts to "Queue".
        let episode2UUID = "A1A1FFFF-0001-0002-0001-000000000002"
        let enclosure2URL = "https://test.podcast.local/episodes/ep2.mp3"

        // When --UITestSeedOrphanClip is present, write clips.json directly.
        //
        // Architecture note: ClipsState (kernel substate) loads exclusively from
        // clips.json at set_data_dir time (data_dir.rs:159). Clips embedded in
        // podcasts.json are only loaded into PodcastStore.clips, which is NOT
        // the source the snapshot builder reads (ffi/snapshot.rs:106 uses
        // state.clips.project, not store.clips). Therefore we must write
        // clips.json — not add a "clips" array to podcasts.json.
        //
        // The ClipRecord JSON shape (clip_handler.rs) is what clips.json holds:
        //   id, episode_id, episode_title, podcast_title, start_secs, end_secs,
        //   title (Option), transcript_text, speaker (Option), source,
        //   refinement_status, auto_snip_anchor_secs (Option), created_at.
        //
        // The clip's episode_id is a fixed UUID NOT in the seeded episodes list
        // — that's what makes it "orphan". project_clips falls back to
        // ClipRecord.episode_title when the episode isn't in the library
        // (clip_handler.rs lookup_titles), so the clip is still projected.
        // ClippingsView renders unconditionally via ClippingsCard (nil episode
        // accepted), verifying fix 19b46163.
        //
        // created_at is 30 days ago → "Earlier" bucket (>7*86400 s old).
        if CommandLine.arguments.contains("--UITestSeedOrphanClip") {
            let thirtyDaysAgo = Int(Date().timeIntervalSince1970) - 30 * 86_400
            // Compact JSON (no leading whitespace) to avoid any parser quirks.
            let clipsJSON = "[{\"id\":\"05480548-0548-0548-0548-054800000001\",\"episode_id\":\"deadbeef-dead-dead-dead-000000000001\",\"episode_title\":\"Orphaned Episode\",\"podcast_title\":\"This American Life\",\"start_secs\":60.0,\"end_secs\":90.0,\"title\":\"Orphan clip\",\"transcript_text\":\"economy is not going\",\"speaker\":null,\"source\":\"touch\",\"refinement_status\":\"manual\",\"auto_snip_anchor_secs\":null,\"created_at\":\(thirtyDaysAgo)}]"
            let clipsFile = dir.appendingPathComponent("clips.json")
            // Remove any stale clips.json from a prior run so the kernel
            // always finds our fresh seed and not a cached empty list.
            try? FileManager.default.removeItem(at: clipsFile)
            if let data = clipsJSON.data(using: .utf8) {
                do {
                    try data.write(to: clipsFile, options: .atomic)
                    NSLog("UITestSeeder: wrote clips.json (\(data.count) bytes) at \(clipsFile.path)")
                } catch {
                    NSLog("UITestSeeder: FAILED to write clips.json: \(error)")
                }
            } else {
                NSLog("UITestSeeder: FAILED to encode clipsJSON as UTF-8")
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
            },{
              "id": "\(episode2UUID.lowercased())",
              "podcast_id": "a1a1ffff-0001-0001-0001-000000000001",
              "guid": "37537 at https://www.thisamericanlife.org",
              "title": "136: Once More with Feeling",
              "description": "People who tried something a second time.",
              "pub_date": "2026-04-01T00:00:00Z",
              "duration_secs": 240.0,
              "enclosure_url": "\(enclosure2URL)",
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
          "local_paths": [["\(episodeUUID.lowercased())", \(localPathLiteral)]],
          "file_sizes": [["\(episodeUUID.lowercased())", \(localBytes)]],
          "settings": {},
          "queue": [],
          "pending_wifi_downloads": []
        }
        """
        try? seed.data(using: .utf8)?.write(to: file, options: .atomic)
    }

    private static func jsonStringLiteral(_ value: String) -> String {
        guard let data = try? JSONEncoder().encode(value),
              let encoded = String(data: data, encoding: .utf8)
        else { return "\"\"" }
        return encoded
    }
}
