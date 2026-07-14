import XCTest
@testable import Podcastr

/// Coverage for the unified `list_episodes` tool surface. Three input shapes:
///   - Internal UUID `podcast_id` → reads straight from the inventory adapter.
///   - Numeric `podcast_id` (iTunes collection_id) → resolves to feed_url via
///     the directory, then captures metadata + episodes via
///     `subscribe.ensurePodcast` WITHOUT creating a `PodcastSubscription`.
///   - `feed_url` directly → same as above but skips the directory hop.
///
/// All external paths must NOT call `subscribe.subscribe` — that would flip
/// the follow bit against the user's intent, which is the whole bug this
/// unification fixes.
@MainActor
final class AgentToolsListEpisodesTests: XCTestCase {

    func testRequiresOneArg() async throws {
        let result = await dispatch(name: "list_episodes", args: [:], inventory: MockInventory())
        let json = try unwrapJSON(result)
        let err = json["error"] as? String ?? ""
        XCTAssertTrue(err.contains("Provide one of"))
    }

    func testRejectsBothArgs() async throws {
        let result = await dispatch(
            name: "list_episodes",
            args: ["podcast_id": "abc", "feed_url": "https://example.com/feed.xml"],
            inventory: MockInventory()
        )
        let json = try unwrapJSON(result)
        let err = json["error"] as? String ?? ""
        XCTAssertTrue(err.contains("Provide only one"))
    }

    func testInternalUUIDPathReturnsErrorForUnknownPodcast() async throws {
        // A well-formed UUID that the inventory doesn't know about routes
        // through the internal path and reports the same "Unknown podcast"
        // error the tool surfaced before this change.
        let unknownUUID = UUID().uuidString
        let result = await dispatch(name: "list_episodes", args: ["podcast_id": unknownUUID], inventory: MockInventory())
        let json = try unwrapJSON(result)
        let err = json["error"] as? String ?? ""
        XCTAssertTrue(err.contains("Unknown podcast"))
    }

    func testInternalUUIDPathReturnsEpisodes() async throws {
        // Existing behavior preserved: a UUID podcast_id reads directly
        // from the inventory adapter, no directory / ensure round-trip.
        let podcastUUID = UUID().uuidString
        let inventory = MockInventory()
        await inventory.setEpisodes([
            sampleEpisode(id: "e1", podcast: podcastUUID, played: false, position: 0),
            sampleEpisode(id: "e2", podcast: podcastUUID, played: true, position: 0),
            sampleEpisode(id: "e3", podcast: podcastUUID, played: false, position: 1234),
        ], forPodcast: podcastUUID)
        let directory = MockDirectory()
        let subscribe = MockSubscribe()

        let result = await dispatch(
            name: "list_episodes",
            args: ["podcast_id": podcastUUID],
            inventory: inventory,
            directory: directory,
            subscribe: subscribe
        )

        let json = try unwrapJSON(result)
        XCTAssertEqual(json["success"] as? Bool, true)
        let rows = json["episodes"] as? [[String: Any]] ?? []
        XCTAssertEqual(rows.count, 3)
        XCTAssertEqual(rows[0]["played"] as? Bool, false)
        XCTAssertEqual(rows[1]["played"] as? Bool, true)
        XCTAssertEqual(rows[2]["is_in_progress"] as? Bool, true)
        XCTAssertEqual(rows[2]["playback_position_seconds"] as? Double, 1234)
        // Internal path must NOT touch the directory or the subscribe service.
        let lookupCalls = await directory.lookupCalls
        let ensureCalls = await subscribe.ensureCalls
        XCTAssertEqual(lookupCalls, [])
        XCTAssertEqual(ensureCalls, [])
    }

    func testCollectionIDPathResolvesViaDirectoryAndEnsures() async throws {
        // Stub directory to resolve the iTunes collection_id to a feed URL,
        // and stub subscribe.ensurePodcast to land a Podcast row that the
        // inventory mock already has episodes for. The handler should:
        //   1. NOT call subscribe.subscribe (no follow flip).
        //   2. Hand back the ensured podcast_id + episodes.
        let collectionID = "863897795"
        let feedURL = "https://example.com/lex.xml"
        let ensuredPodcastID = "ensured-pod-1"

        let directory = MockDirectory()
        await directory.setFeedURL(feedURL, forCollectionID: collectionID)

        let subscribe = MockSubscribe()
        await subscribe.setEnsureResult(
            PodcastEnsureResult(
                podcastID: ensuredPodcastID,
                title: "Lex",
                author: "Lex Fridman",
                feedURL: feedURL,
                episodeCount: 2
            ),
            forFeedURL: feedURL
        )

        let inventory = MockInventory()
        await inventory.setEpisodes([
            sampleEpisode(id: "e1", podcast: ensuredPodcastID, played: false, position: 0),
            sampleEpisode(id: "e2", podcast: ensuredPodcastID, played: false, position: 0),
        ], forPodcast: ensuredPodcastID)

        let result = await dispatch(
            name: "list_episodes",
            args: ["podcast_id": collectionID],
            inventory: inventory,
            directory: directory,
            subscribe: subscribe
        )

        let json = try unwrapJSON(result)
        XCTAssertEqual(json["success"] as? Bool, true)
        XCTAssertEqual(json["podcast_id"] as? String, ensuredPodcastID)
        XCTAssertEqual(json["feed_url"] as? String, feedURL)
        XCTAssertEqual(json["podcast_title"] as? String, "Lex")
        let rows = json["episodes"] as? [[String: Any]] ?? []
        XCTAssertEqual(rows.count, 2)

        let lookupCalls = await directory.lookupCalls
        XCTAssertEqual(lookupCalls, [collectionID])
        let ensureCalls = await subscribe.ensureCalls
        XCTAssertEqual(ensureCalls, [feedURL])
        // CRITICAL: ensurePodcast must NOT flip the follow bit. The agent
        // tool surface never invokes `subscribe.subscribe` on this path.
        let subscribeCalls = await subscribe.subscribeCalls
        XCTAssertEqual(subscribeCalls, [])
    }

    func testFeedURLPathSkipsDirectoryLookup() async throws {
        let feedURL = "https://example.com/feed.xml"
        let ensuredPodcastID = "ensured-pod-2"

        let directory = MockDirectory()
        let subscribe = MockSubscribe()
        await subscribe.setEnsureResult(
            PodcastEnsureResult(
                podcastID: ensuredPodcastID,
                title: "Feed Show",
                feedURL: feedURL,
                episodeCount: 1
            ),
            forFeedURL: feedURL
        )

        let inventory = MockInventory()
        await inventory.setEpisodes([
            sampleEpisode(id: "e1", podcast: ensuredPodcastID, played: false, position: 0),
        ], forPodcast: ensuredPodcastID)

        let result = await dispatch(
            name: "list_episodes",
            args: ["feed_url": feedURL],
            inventory: inventory,
            directory: directory,
            subscribe: subscribe
        )

        let json = try unwrapJSON(result)
        XCTAssertEqual(json["success"] as? Bool, true)
        XCTAssertEqual(json["podcast_id"] as? String, ensuredPodcastID)
        XCTAssertEqual(json["feed_url"] as? String, feedURL)
        let rows = json["episodes"] as? [[String: Any]] ?? []
        XCTAssertEqual(rows.count, 1)
        // Directory lookup must NOT fire on the feed_url path.
        let lookupCalls = await directory.lookupCalls
        XCTAssertEqual(lookupCalls, [])
        let ensureCalls = await subscribe.ensureCalls
        XCTAssertEqual(ensureCalls, [feedURL])
        let subscribeCalls = await subscribe.subscribeCalls
        XCTAssertEqual(subscribeCalls, [])
    }

    func testCollectionIDUnresolvedReturnsError() async throws {
        // The directory returns nil for the given collection_id → tool
        // surfaces a clear error verbatim, no fallback to "no episodes."
        let collectionID = "999999999"
        let directory = MockDirectory()
        // No feed URL configured → lookupFeedURL returns nil.
        let subscribe = MockSubscribe()

        let result = await dispatch(
            name: "list_episodes",
            args: ["podcast_id": collectionID],
            inventory: MockInventory(),
            directory: directory,
            subscribe: subscribe
        )

        let json = try unwrapJSON(result)
        let err = json["error"] as? String ?? ""
        XCTAssertTrue(err.contains("Could not resolve podcast directory ID"))
        XCTAssertTrue(err.contains(collectionID))
        let ensureCalls = await subscribe.ensureCalls
        XCTAssertEqual(ensureCalls, [])
    }

    func testFeedURLEnsureFailureSurfacesError() async throws {
        // `ensurePodcast` throws (e.g. network / parse). Tool surfaces the
        // error rather than masking it as "no episodes."
        struct StubError: LocalizedError {
            var errorDescription: String? { "feed not reachable" }
        }
        let subscribe = MockSubscribe()
        await subscribe.setEnsureError(StubError())

        let result = await dispatch(
            name: "list_episodes",
            args: ["feed_url": "https://broken.example.com/feed.xml"],
            inventory: MockInventory(),
            directory: MockDirectory(),
            subscribe: subscribe
        )

        let json = try unwrapJSON(result)
        let err = json["error"] as? String ?? ""
        XCTAssertTrue(err.contains("Could not load feed"))
        XCTAssertTrue(err.contains("feed not reachable"))
    }

    // MARK: - Helpers

    private func dispatch(
        name: String,
        args: [String: Any],
        inventory: MockInventory,
        directory: MockDirectory = MockDirectory(),
        subscribe: MockSubscribe = MockSubscribe()
    ) async -> String {
        let deps = PodcastAgentToolDeps(
            rag: MockRAG(),
            wiki: MockWiki(),
            briefing: MockBriefing(),
            summarizer: MockSummarizer(),
            fetcher: MockFetcher(),
            playback: MockPlayback(),
            library: MockLibrary(),
            inventory: inventory,
            categories: inventory,
            peerPublisher: MockPeerEventPublisher(),
            friendDirectory: MockFriendDirectory(),
            perplexity: MockPerplexity(),
            ttsPublisher: MockTTSPublisher(),
            directory: directory,
            subscribe: subscribe,
            youtubeIngestion: MockYouTubeIngestion(),
            ownedPodcasts: MockOwnedPodcasts()
        )
        let argsJSON: String
        if args.isEmpty {
            argsJSON = "{}"
        } else {
            let data = (try? JSONSerialization.data(withJSONObject: args)) ?? Data("{}".utf8)
            argsJSON = String(data: data, encoding: .utf8) ?? "{}"
        }
        return await AgentTools.dispatchPodcast(name: name, argsJSON: argsJSON, deps: deps)
    }

    private func unwrapJSON(_ result: String) throws -> [String: Any] {
        let data = try XCTUnwrap(result.data(using: .utf8))
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func sampleEpisode(
        id: String,
        podcast: String,
        played: Bool,
        position: Double
    ) -> EpisodeInventoryRow {
        EpisodeInventoryRow(
            episodeID: id,
            podcastID: podcast,
            title: "Episode \(id)",
            podcastTitle: "Show \(podcast)",
            publishedAt: Date(timeIntervalSince1970: 1_700_000_000),
            durationSeconds: 1800,
            played: played,
            playbackPositionSeconds: position,
            isInProgress: !played && position > 0
        )
    }
}
