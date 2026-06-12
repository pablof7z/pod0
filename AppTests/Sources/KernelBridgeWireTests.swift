import XCTest
@testable import Podcastr

// ─── Rust-shaped fixture helpers ──────────────────────────────────────────────
//
// Each helper produces a `{"t":"snapshot","v":{"projections":{...}}}` envelope
// that faithfully mirrors the JSON `nmp_app_podcast_decode_update_frame` injects.
// All field names are snake_case (the bridge decoder converts them to camelCase).

private func makeEnvelope(projections: [String: Any]) -> Data {
    let body: [String: Any] = [
        "t": "snapshot",
        "v": ["projections": projections]
    ]
    return try! JSONSerialization.data(withJSONObject: body)
}

private func playbackProjection(rev: Int, nowPlayingId: String? = "ep-1") -> [String: Any] {
    var np: [String: Any]? = nil
    if let id = nowPlayingId {
        np = [
            "episode_id": id,
            "podcast_id": "pod-1",
            "position_secs": 42.0,
            "duration_secs": 1800.0,
            "is_playing": true
        ]
    }
    var proj: [String: Any] = ["rev": rev, "queue": []]
    proj["now_playing"] = np as Any
    return proj
}

private func libraryProjection(rev: Int, podcastTitle: String = "The Daily") -> [String: Any] {
    [
        "rev": rev,
        "library": [[
            "id": "pod-1",
            "title": podcastTitle,
            "episodes": [],
            "is_subscribed": true,
            "episode_count": 0,
            "unplayed_count": 0,
            "nostr_visibility": ""
        ]],
        "categories": [],
        "search_results": [],
        "nostr_results": [],
        "owned_podcasts": [],
        "inbox": [],
        "inbox_triage_in_progress": false
    ]
}

private func identityProjection(rev: Int, npub: String = "npub1test") -> [String: Any] {
    [
        "rev": rev,
        "active_account": [
            "npub": npub,
            "pubkey_hex": "deadbeef",
            "mode": "local"
        ]
    ]
}

private func settingsProjection(rev: Int) -> [String: Any] {
    ["rev": rev, "settings": [:] as [String: Any], "configured_relays": []]
}

final class KernelBridgeWireTests: XCTestCase {
    func testFeedActionPayloadsEncodeRustWireShape() throws {
        XCTAssertEqual(
            try PodcastKernelAction.Subscribe(feedUrl: "https://example.com/feed.xml")
                .bodyDictionary()["op"] as? String,
            "subscribe"
        )
        XCTAssertEqual(
            try PodcastKernelAction.EnsurePodcast(feedUrl: "https://example.com/feed.xml")
                .bodyDictionary()["op"] as? String,
            "ensure_podcast"
        )
        XCTAssertEqual(
            try PodcastKernelAction.RefreshAll().bodyDictionary()["op"] as? String,
            "refresh_all"
        )

        let refresh = try PodcastKernelAction.Refresh(podcastId: "pod-1").bodyDictionary()
        XCTAssertEqual(refresh["op"] as? String, "refresh")
        XCTAssertEqual(refresh["podcast_id"] as? String, "pod-1")

        let unsubscribe = try PodcastKernelAction.Unsubscribe(podcastId: "pod-1").bodyDictionary()
        XCTAssertEqual(unsubscribe["op"] as? String, "unsubscribe")
        XCTAssertEqual(unsubscribe["podcast_id"] as? String, "pod-1")
    }

    func testCreatePodcastOmitsNilOptionalFields() throws {
        let body = try PodcastKernelAction.CreatePodcast(
            podcastId: "pod-1",
            title: "Agent Show",
            description: "",
            author: "",
            feedUrl: nil,
            artworkUrl: nil,
            language: nil,
            categories: [],
            visibility: "private",
            titleIsPlaceholder: false
        ).bodyDictionary()

        XCTAssertEqual(body["op"] as? String, "create_podcast")
        XCTAssertEqual(body["podcast_id"] as? String, "pod-1")
        XCTAssertEqual(body["description"] as? String, "")
        XCTAssertEqual(body["author"] as? String, "")
        XCTAssertEqual(body["categories"] as? [String], [])
        XCTAssertNil(body["feed_url"])
        XCTAssertNil(body["artwork_url"])
        XCTAssertNil(body["language"])
        XCTAssertEqual(body["visibility"] as? String, "private")
        XCTAssertEqual(body["title_is_placeholder"] as? Bool, false)
    }

    func testAddEpisodeEncodesTypedChaptersAndOmitsNilOptionalFields() throws {
        let body = try PodcastKernelAction.AddEpisode(
            podcastId: "pod-1",
            episodeId: "ep-1",
            title: "Episode",
            enclosureUrl: "https://example.com/audio.mp3",
            description: "",
            durationSecs: nil,
            imageUrl: nil,
            chapters: [
                KernelEpisodeChapterPayload(
                    startSecs: 12.5,
                    title: "Clip",
                    imageUrl: "https://example.com/art.png",
                    sourceEpisodeId: "source-ep"
                )
            ],
            transcript: nil
        ).bodyDictionary()

        XCTAssertEqual(body["op"] as? String, "add_episode")
        XCTAssertEqual(body["enclosure_url"] as? String, "https://example.com/audio.mp3")
        XCTAssertNil(body["duration_secs"])
        XCTAssertNil(body["image_url"])
        XCTAssertNil(body["transcript"])
        let chapters = body["chapters"] as? [[String: Any]]
        XCTAssertEqual(chapters?.count, 1)
        XCTAssertEqual(chapters?.first?["start_secs"] as? Double, 12.5)
        XCTAssertEqual(chapters?.first?["source_episode_id"] as? String, "source-ep")
    }

    func testPodcastSummaryDecodesSubscriptionAndRefreshFields() throws {
        let data = Data("""
        {
          "id": "pod-1",
          "title": "Known Show",
          "is_subscribed": false,
          "last_refreshed_at": 1767225600000,
          "title_is_placeholder": true,
          "episodes": []
        }
        """.utf8)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase

        let summary = try decoder.decode(PodcastSummary.self, from: data)

        XCTAssertFalse(summary.isSubscribed)
        XCTAssertEqual(summary.lastRefreshedAt, 1_767_225_600_000)
        XCTAssertTrue(summary.titleIsPlaceholder)
    }

    // ── Per-domain wire decode tests (the #371/#384 lesson) ───────────────────

    /// A playback-domain frame decodes through the KernelDecoding seam
    /// (snake_case → camelCase) and populates nowPlaying.episodeId.
    func testPlaybackDomainFrameDecodesViaKernelDecodingSeam() throws {
        let data = makeEnvelope(projections: [
            DomainSchema.playback: playbackProjection(rev: 1)
        ])
        let frames = try XCTUnwrap(
            PodcastDomainFrames.decode(from: data),
            "frame with podcast.playback sidecar must yield a non-nil PodcastDomainFrames")
        let play = try XCTUnwrap(frames.playback, "playback domain must be non-nil")
        XCTAssertEqual(play.rev, 1)
        // snake_case `episode_id` → camelCase `episodeId` via .convertFromSnakeCase
        XCTAssertEqual(play.nowPlaying?.episodeId, "ep-1")
        XCTAssertEqual(play.nowPlaying?.positionSecs, 42.0)
        // Library domain absent in a playback-only frame.
        XCTAssertNil(frames.library, "library domain must be absent from a playback-only frame")
    }

    /// A library-domain frame decodes and populates the library slice.
    func testLibraryDomainFrameDecodesViaKernelDecodingSeam() throws {
        let data = makeEnvelope(projections: [
            DomainSchema.library: libraryProjection(rev: 2, podcastTitle: "My Show")
        ])
        let frames = try XCTUnwrap(PodcastDomainFrames.decode(from: data))
        let lib = try XCTUnwrap(frames.library)
        XCTAssertEqual(lib.rev, 2)
        XCTAssertEqual(lib.library?.first?.title, "My Show")
        // isSubscribed decodes snake_case via .convertFromSnakeCase
        XCTAssertEqual(lib.library?.first?.isSubscribed, true)
        // Other domains absent.
        XCTAssertNil(frames.playback)
        XCTAssertNil(frames.settings)
    }

    /// An identity-domain frame decodes and the active_account fields survive
    /// the snake_case → camelCase conversion.
    func testIdentityDomainFrameDecodesActiveAccountViaSnakeCaseConversion() throws {
        let data = makeEnvelope(projections: [
            DomainSchema.identity: identityProjection(rev: 3, npub: "npub1abc")
        ])
        let frames = try XCTUnwrap(PodcastDomainFrames.decode(from: data))
        let ident = try XCTUnwrap(frames.identity)
        XCTAssertEqual(ident.rev, 3)
        // pubkey_hex → pubkeyHex
        XCTAssertEqual(ident.activeAccount?.pubkeyHex, "deadbeef")
        XCTAssertEqual(ident.activeAccount?.npub, "npub1abc")
    }

    /// A frame with multiple domains decodes all present domains independently.
    func testMultiDomainFrameDecodesAllPresentDomains() throws {
        let data = makeEnvelope(projections: [
            DomainSchema.playback: playbackProjection(rev: 10),
            DomainSchema.library:  libraryProjection(rev: 5),
            DomainSchema.settings: settingsProjection(rev: 1)
        ])
        let frames = try XCTUnwrap(PodcastDomainFrames.decode(from: data))
        XCTAssertNotNil(frames.playback, "playback domain must be present")
        XCTAssertNotNil(frames.library,  "library domain must be present")
        XCTAssertNotNil(frames.settings, "settings domain must be present")
        XCTAssertNil(frames.identity,    "identity domain must be absent")
        XCTAssertNil(frames.widget,      "widget domain must be absent")
    }

    /// A playback-only push frame merging into a composite DOES NOT clear the
    /// library slice — the library domain is absent, so the composite library
    /// is preserved. Core delta-transport correctness assertion.
    func testPlaybackOnlyFrameDoesNotClearLibrarySliceInComposite() throws {
        // Prime the composite with library data via a library domain frame.
        let libData = makeEnvelope(projections: [
            DomainSchema.library: libraryProjection(rev: 1)
        ])
        let libFrames = try XCTUnwrap(PodcastDomainFrames.decode(from: libData))
        var composite = PodcastUpdate()
        var tracker = KernelModel.DomainRevTracker()

        // Simulate the merge helper directly (not KernelModel — it's @MainActor).
        // Accept the library domain.
        if let lib = libFrames.library, lib.rev > tracker.library {
            tracker.library = lib.rev
            composite.library = lib.library ?? []
        }
        XCTAssertEqual(composite.library.count, 1,
                       "composite must carry one show after library domain merge")

        // Now merge a playback-only frame.
        let playData = makeEnvelope(projections: [
            DomainSchema.playback: playbackProjection(rev: 2)
        ])
        let playFrames = try XCTUnwrap(PodcastDomainFrames.decode(from: playData))
        // Playback is present; library is absent (nil) → must NOT touch composite.library.
        XCTAssertNil(playFrames.library,
                     "library domain must be absent from a playback-only frame")
        if let play = playFrames.playback, play.rev > tracker.playback {
            tracker.playback = play.rev
            composite.nowPlaying = play.nowPlaying
        }
        // Library slice must survive untouched.
        XCTAssertEqual(composite.library.count, 1,
                       "library slice must survive a playback-only push frame (delta-transport)")
        XCTAssertEqual(composite.nowPlaying?.episodeId, "ep-1",
                       "nowPlaying must be updated from the playback domain")
    }

    /// A tombstone identity frame (`active_account: null`) clears the identity
    /// slice in the domain frame (activeAccount == nil).
    func testIdentityTombstoneFrameClearsActiveAccount() throws {
        // A tombstone: the kernel emits the identity domain with active_account null.
        let tombstoneProjection: [String: Any] = [
            "rev": 99,
            "active_account": NSNull()
        ]
        let data = makeEnvelope(projections: [
            DomainSchema.identity: tombstoneProjection
        ])
        let frames = try XCTUnwrap(PodcastDomainFrames.decode(from: data))
        let ident = try XCTUnwrap(frames.identity,
                                  "identity domain must decode even with null active_account")
        XCTAssertEqual(ident.rev, 99)
        XCTAssertNil(ident.activeAccount,
                     "tombstone frame must decode active_account as nil (logged-out state)")
    }

    /// A playback domain frame whose rev is ≤ the tracker's last-applied rev is
    /// dropped without touching the composite (stale/out-of-order protection).
    func testDropGuardIgnoresStaleRevFrame() throws {
        var composite = PodcastUpdate()
        var tracker = KernelModel.DomainRevTracker()

        // Accept rev=5.
        let highRevData = makeEnvelope(projections: [
            DomainSchema.playback: playbackProjection(rev: 5, nowPlayingId: "ep-current")
        ])
        let highFrames = try XCTUnwrap(PodcastDomainFrames.decode(from: highRevData))
        if let play = highFrames.playback, play.rev > tracker.playback {
            tracker.playback = play.rev
            composite.nowPlaying = play.nowPlaying
        }
        XCTAssertEqual(composite.nowPlaying?.episodeId, "ep-current")

        // Arrive a stale rev=3 frame (simulating out-of-order delivery).
        let staleData = makeEnvelope(projections: [
            DomainSchema.playback: playbackProjection(rev: 3, nowPlayingId: "ep-stale")
        ])
        let staleFrames = try XCTUnwrap(PodcastDomainFrames.decode(from: staleData))
        // Drop-guard: 3 <= 5, must NOT merge.
        var mergedStale = false
        if let play = staleFrames.playback, play.rev > tracker.playback {
            tracker.playback = play.rev
            composite.nowPlaying = play.nowPlaying
            mergedStale = true
        }
        XCTAssertFalse(mergedStale, "stale-rev domain frame must be dropped by the drop-guard")
        XCTAssertEqual(composite.nowPlaying?.episodeId, "ep-current",
                       "composite must retain the higher-rev state after a stale-rev drop")
    }

    /// A frame with no `podcast.*` projections yields nil — D6 degrade.
    func testAbsentDomainFrameYieldsNil() {
        let data = makeEnvelope(projections: [:])
        XCTAssertNil(
            PodcastDomainFrames.decode(from: data),
            "frame with no podcast.* projections must yield nil (D6 degrade)")
    }

    /// `DomainSchema` constants match the Rust schema IDs the kernel injects.
    func testDomainSchemaConstantsMatchKernelSchemaIDs() {
        XCTAssertEqual(DomainSchema.library,   "podcast.library")
        XCTAssertEqual(DomainSchema.playback,  "podcast.playback")
        XCTAssertEqual(DomainSchema.downloads, "podcast.downloads")
        XCTAssertEqual(DomainSchema.settings,  "podcast.settings")
        XCTAssertEqual(DomainSchema.identity,  "podcast.identity")
        XCTAssertEqual(DomainSchema.widget,    "podcast.widget")
        XCTAssertEqual(DomainSchema.misc,      "podcast.misc")
    }
}
