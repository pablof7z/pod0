import Foundation

/// Writes a minimal This American Life library seed to the kernel's
/// `podcasts.json` when the app is launched with `--UITestSeed`.
///
/// Called from `AppDelegate.didFinishLaunchingWithOptions` (before the kernel
/// opens the store in `KernelModel.start()`). The write is synchronous; the
/// kernel reads `podcasts.json` at data-dir binding / `PodcastApp.start` time, so the
/// seed must land before that call. Two modes:
///   • `--UITestSeed` (fresh): always overwrites `podcasts.json` with a
///     known-good seed and wipes the Swift SQLite metadata sidecar, so every
///     test starts from a clean, position-0 state.
///   • `--UITestSeedRelaunch`: preserves the kernel's existing `podcasts.json`
///     (which already carries the position the kernel persisted last session)
///     and wipes only the SQLite sidecar, proving resume comes from the kernel
///     (position is kernel-owned, never stored in SQLite — #561).
///
/// Never compiled out — the `CommandLine.arguments` guard is the safety valve
/// so this is a no-op in production. Kept in the main target (not the test
/// target) because it must run inside the app process where it has access to
/// `applicationSupportDirectory`.
enum UITestSeeder {
    static let primaryEpisodeDurationSecs: Double = 300.0

    static func seededDownloadURL(episodeID: String, sourceURL: URL?) -> URL {
        DownloadCapability.destinationURL(
            for: episodeID,
            sourceURL: sourceURL,
            kind: .episode
        )
    }

    static func seedIfNeeded() {
        guard CommandLine.arguments.contains("--UITestSeed") else { return }
        guard let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask).first
        else { return }
        let dir = base.appendingPathComponent("PodcastLibrary", isDirectory: true)
        let file = dir.appendingPathComponent("podcasts.json")

        // On a relaunch test (--UITestSeedRelaunch), the kernel's podcasts.json
        // already carries the persisted position_secs written by apply_writeback
        // during the previous session. Preserving that file is the whole point:
        // the resume test must pass via the kernel's own persistence, independent
        // of any Swift SQLite write. We skip overwriting podcasts.json entirely
        // and let the kernel reload the position it wrote itself.
        if CommandLine.arguments.contains("--UITestSeedRelaunch") {
            // Wipe Swift SQLite so the old AppStateStore data doesn't interfere.
            Persistence.shared.reset()
            try? Persistence.shared.episodeStore.replaceAll([])
            NSLog("UITestSeeder: relaunch — preserving kernel podcasts.json, SQLite wiped")
            return
        }

        // Non-relaunch: wipe the iCloud KV store settings so iCloudSyncCapability
        // does not restore stale values (e.g. default_playback_rate=1.5 from a
        // prior testPlaybackSpeedPersists run) that would override the fresh seed.
        // All pcst.* keys are removed so every setting starts from the kernel
        // default, matching what the fresh podcasts.json seed provides.
        let kvs = NSUbiquitousKeyValueStore.default
        let pcstPrefix = "pcst."
        for key in kvs.dictionaryRepresentation.keys where key.hasPrefix(pcstPrefix) {
            kvs.removeObject(forKey: key)
        }
        NSLog("UITestSeeder: cleared all pcst.* iCloud KV keys")

        // Non-relaunch: write a fresh known-good seed so every test starts clean.
        // Always overwrite: the kernel may have replaced a prior seed with real
        // RSS data or a stale seed from a previous run.
        try? FileManager.default.removeItem(at: file)
        // Also wipe the kernel-owned clips sidecar on EVERY --UITestSeed so a
        // prior run's seeded clip (e.g. the orphan clip below) can't contaminate
        // a later test that seeds without --UITestSeedOrphanClip. The orphan
        // branch rewrites clips.json afterwards when that flag IS present.
        try? FileManager.default.removeItem(at: dir.appendingPathComponent("clips.json"))
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        // Wipe the Swift persistence store so stale episodes/metadata from a
        // prior test don't bleed into this run via Persistence.load()'s
        // "JSON absent → hydrate from SQLite" path. Playback position is NOT in
        // SQLite (kernel-owned in podcasts.json via apply_writeback, #561), so
        // this wipe is about a clean episode list, not position.
        Persistence.shared.reset()
        try? Persistence.shared.episodeStore.replaceAll([])
        let episodeUUID = "A1A1FFFF-0001-0002-0001-000000000001"
        let enclosureURL = "https://npr.simplecastaudio.com/d3081dd9-fcaf-445a-977c-4f56c28f5a6e/episodes/e55b1946-2658-4592-9afe-1c2a3033a31c/audio/128/default.mp3"
        let sourceURL = URL(string: enclosureURL)
        let destMP3 = seededDownloadURL(episodeID: episodeUUID, sourceURL: sourceURL)

        // Copy the bundled test MP3 to the canonical download path so AVPlayer
        // plays from disk (reliable in the simulator) rather than streaming from
        // the NPR CDN (which the simulator's sandboxed network may block).
        // ep1 is always seeded as downloaded so every playback-dependent UI test
        // (resume-across-restart, queue play, chapter seek, playback speed) has a
        // working local file. ep2 and ep3 stay not_downloaded so testDownloadEpisode
        // can target a genuinely not_downloaded episode without depending on ep1.
        let localBytes = installSeededEpisodeAudio(to: destMP3)
        let localPathsJSON = "[[\"\(episodeUUID.lowercased())\", \(jsonStringLiteral(destMP3.path))]]"
        let fileSizesJSON = "[[\"\(episodeUUID.lowercased())\", \(localBytes)]]"
        // Seed ep1 download_state as downloaded so the UI shows the "Downloaded"
        // pill without requiring a real download. local_file_url + byte_count must
        // match the actual file so the kernel's projection is self-consistent.
        let ep1DownloadState = """
        {"state": "downloaded", "local_file_url": \(jsonStringLiteral(destMP3.absoluteString)), "byte_count": \(localBytes)}
        """

        // Fresh-seed: position starts at 0.
        let persistedPosition: Double = 0.0

        // ep2 UUID — seeded into the kernel library so the Queue action is accepted
        // and the authoritative projection includes it. Without this entry the
        // kernel silently drops the ep2 enqueue, the snapshot clobbers the
        // optimistic local state, and the "Queued" label reverts to "Queue".
        let episode2UUID = "A1A1FFFF-0001-0002-0001-000000000002"
        let enclosure2URL = "https://test.podcast.local/episodes/ep2.mp3"

        // ep3 UUID — third episode used by the queue-reorder test. Always seeded
        // so the kernel knows the episode. Pub-date earlier than ep2 so the
        // show-detail list order is ep1 (index 0), ep2 (index 1), ep3 (index 2).
        let episode3UUID = "A1A1FFFF-0001-0002-0001-000000000003"
        let enclosure3URL = "https://test.podcast.local/episodes/ep3.mp3"

        // Queue is always empty at seed time; the queue-reorder test builds the
        // queue through the UI rather than depending on a pre-seeded queue state.
        let queueJSON = "[]"

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

        // ep1 chapters — publisher-supplied, always seeded so PlayerChaptersUITests
        // can run without a network fetch. Three fixed UUIDs for stable references
        // in tests; the kernel projects them as ChapterSummary → Episode.Chapter
        // (see snapshot_library.rs). The Swift bridge generates a fresh UUID per
        // projected chapter, so the a11y id is "chapter-<random-uuid>" — tests
        // match with BEGINSWITH 'chapter-' rather than a specific UUID.
        let ep1Chapters = """
        [
          {"id":"c0010001-0001-0001-0001-000000000001","start_secs":0.0,"end_secs":60.0,
           "title":"Introduction","include_in_toc":true,"is_ai_generated":false},
          {"id":"c0010001-0001-0001-0001-000000000002","start_secs":60.0,"end_secs":180.0,
           "title":"Main Story","include_in_toc":true,"is_ai_generated":false},
          {"id":"c0010001-0001-0001-0001-000000000003","start_secs":180.0,
           "title":"Conclusion","include_in_toc":true,"is_ai_generated":false}
        ]
        """

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
              "duration_secs": \(Self.primaryEpisodeDurationSecs),
              "enclosure_url": "\(enclosureURL)",
              "enclosure_mime_type": "audio/mpeg",
              "position_secs": \(persistedPosition),
              "played": false,
              "is_starred": false,
              "download_state": \(ep1DownloadState),
              "transcript_state": {"state": "none"},
              "triage_is_hero": false,
              "metadata_indexed": false,
              "chapters": \(ep1Chapters)
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
            },{
              "id": "\(episode3UUID.lowercased())",
              "podcast_id": "a1a1ffff-0001-0001-0001-000000000001",
              "guid": "37538 at https://www.thisamericanlife.org",
              "title": "135: Deep Space",
              "description": "Exploring the cosmos.",
              "pub_date": "2026-03-01T00:00:00Z",
              "duration_secs": 210.0,
              "enclosure_url": "\(enclosure3URL)",
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
          "local_paths": \(localPathsJSON),
          "file_sizes": \(fileSizesJSON),
          "settings": {"default_playback_rate": 1.0},
          "queue": \(queueJSON),
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

    static func installSeededEpisodeAudio(
        from bundledMP3: URL? = Bundle.main.url(forResource: "test-episode", withExtension: "mp3"),
        to destMP3: URL
    ) -> Int64 {
        try? FileManager.default.createDirectory(
            at: destMP3.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? FileManager.default.removeItem(at: destMP3)
        if let bundledMP3 {
            do { try FileManager.default.copyItem(at: bundledMP3, to: destMP3) }
            catch { NSLog("UITestSeeder: FAILED to install test episode audio: \(error)") }
        } else {
            NSLog("UITestSeeder: missing bundled test-episode.mp3")
        }
        let attrs = try? FileManager.default.attributesOfItem(atPath: destMP3.path)
        return (attrs?[.size] as? NSNumber)?.int64Value ?? 0
    }
}
