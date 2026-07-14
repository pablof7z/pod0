import XCTest
@testable import Podcastr

/// Lane-10 tests. Drive `AgentTools.dispatchPodcast` against mock deps and
/// verify (a) schema validation rejects bad arguments cleanly and (b) the
/// dispatch path forwards to the right protocol method with the right values.
@MainActor
final class AgentToolsPodcastTests: XCTestCase {

    // MARK: - Schema sanity

    func testPodcastSchemaListsEveryNonSkillToolName() {
        let names = Set(AgentTools.podcastSchema.compactMap { tool -> String? in
            (tool["function"] as? [String: Any])?["name"] as? String
        })
        // Skill-gated tool names live in `PodcastNames.all` (so `dispatch` can
        // route them) but their schemas are owned by the skill, not by
        // `podcastSchema`. Peer-only tools live in `peerOnlySchema` and are
        // surfaced only inside Nostr peer conversations. Subtract both before
        // comparing.
        let peerOnlyNames: Set<String> = [
            AgentTools.PodcastNames.endConversation,
        ]
        let expected = Set(AgentTools.PodcastNames.all)
            .subtracting(AgentSkillRegistry.allToolNames)
            .subtracting(peerOnlyNames)
        XCTAssertEqual(names, expected, "podcastSchema must cover every non-skill, non-peer-only podcast tool")
    }

    func testPodcastSchemaEntriesHaveRequiredOpenAIShape() {
        for entry in AgentTools.podcastSchema {
            XCTAssertEqual(entry["type"] as? String, "function")
            let function = entry["function"] as? [String: Any]
            XCTAssertNotNil(function?["name"] as? String)
            XCTAssertNotNil(function?["description"] as? String)
            let params = function?["parameters"] as? [String: Any]
            XCTAssertEqual(params?["type"] as? String, "object")
            XCTAssertNotNil(params?["properties"] as? [String: Any])
            XCTAssertNotNil(params?["required"] as? [String])
        }
    }

    // MARK: - play_episode

    func testPlayEpisodeNowSuccess() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1", "start_seconds": 47.5, "queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        XCTAssertEqual(decoded["episode_id"] as? String, "ep1")
        XCTAssertEqual(decoded["queue_position"] as? String, "now")
        XCTAssertEqual(decoded["started_playing"] as? Bool, true)
        let calls = await deps.playback.recordedPlays
        XCTAssertEqual(calls.count, 1)
        XCTAssertEqual(calls.first?.episodeID, "ep1")
        XCTAssertEqual(calls.first?.startSeconds, 47.5)
        XCTAssertNil(calls.first?.endSeconds)
        XCTAssertEqual(calls.first?.queuePosition, .now)
    }

    func testPlayEpisodeBoundedSegmentNow() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1", "start_seconds": 30.0, "end_seconds": 90.0, "queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        let calls = await deps.playback.recordedPlays
        XCTAssertEqual(calls.first?.startSeconds, 30.0)
        XCTAssertEqual(calls.first?.endSeconds, 90.0)
    }

    func testPlayEpisodeQueueEnd() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1", "queue_position": "end"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["queue_position"] as? String, "end")
        XCTAssertEqual(decoded["started_playing"] as? Bool, false)
        let calls = await deps.playback.recordedPlays
        XCTAssertEqual(calls.first?.queuePosition, .end)
    }

    func testPlayEpisodeQueueNext() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1", "queue_position": "next"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["queue_position"] as? String, "next")
        XCTAssertEqual(decoded["started_playing"] as? Bool, false)
        let calls = await deps.playback.recordedPlays
        XCTAssertEqual(calls.first?.queuePosition, .next)
    }

    func testPlayEpisodeRejectsMissingEpisodeID() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testPlayEpisodeDefaultsQueuePositionToNow() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        XCTAssertEqual(decoded["queue_position"] as? String, "now")
    }

    func testPlayEpisodeRejectsBadQueuePosition() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1", "queue_position": "later"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testPlayEpisodeRejectsUnknownEpisode() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: []))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "does-not-exist", "queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testPlayEpisodeRejectsNegativeStart() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1", "start_seconds": -3.0, "queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testPlayEpisodeRejectsEndBeforeStart() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1", "start_seconds": 50.0, "end_seconds": 20.0, "queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    // MARK: - play_episode (external / audio_url path)

    func testPlayEpisodeExternalURLSuccess() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["audio_url": "https://example.com/ep.mp3", "title": "The Matrix Revisited", "queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        XCTAssertEqual(decoded["queue_position"] as? String, "now")
        XCTAssertEqual(decoded["started_playing"] as? Bool, true)
        XCTAssertEqual(decoded["audio_url"] as? String, "https://example.com/ep.mp3")
        let calls = await deps.playback.recordedExternalPlays
        XCTAssertEqual(calls.count, 1)
        XCTAssertEqual(calls.first?.audioURL.absoluteString, "https://example.com/ep.mp3")
        XCTAssertEqual(calls.first?.title, "The Matrix Revisited")
        XCTAssertNil(calls.first?.feedURLString)
    }

    func testPlayEpisodeExternalURLWithFeedURL() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: [
                "audio_url": "https://example.com/ep.mp3",
                "title": "Guest Appearance",
                "feed_url": "https://feeds.example.com/show.rss",
                "queue_position": "now",
            ],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        XCTAssertEqual(decoded["feed_url"] as? String, "https://feeds.example.com/show.rss")
        let calls = await deps.playback.recordedExternalPlays
        XCTAssertEqual(calls.first?.feedURLString, "https://feeds.example.com/show.rss")
    }

    func testPlayEpisodeExternalURLDefaultsQueuePositionToNow() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["audio_url": "https://example.com/ep.mp3", "title": "Some Episode"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        XCTAssertEqual(decoded["queue_position"] as? String, "now")
    }

    func testPlayEpisodeExternalURLRejectsMissingTitle() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["audio_url": "https://example.com/ep.mp3", "queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
        let calls = await deps.playback.recordedExternalPlays
        XCTAssertEqual(calls.count, 0)
    }

    func testPlayEpisodeRejectsBothEpisodeIDAndAudioURL() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["episode_id": "ep1", "audio_url": "https://example.com/ep.mp3", "title": "Conflict", "queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testPlayEpisodeRejectsMissingBothIdentifiers() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.playEpisode,
            args: ["queue_position": "now"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    // MARK: - Unknown tool

    func testUnknownPodcastToolReturnsError() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: "not_a_real_tool",
            args: [:],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    // MARK: - JSON-from-string entry point

    func testDispatchFromArgsJSONStringParsesObject() async throws {
        // Uses set_playback_rate as a JSON-shape carrier: it requires a numeric
        // `rate` arg, so parsing must succeed for the success envelope to come
        // back with `rate` set.
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.setPlaybackRate,
            argsJSON: #"{"rate":1.5}"#,
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["requested_rate"] as? Double, 1.5)
    }

    func testDispatchFromArgsJSONStringRejectsMalformed() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.setPlaybackRate,
            argsJSON: "not json",
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    // MARK: - PerplexityClient response parsing

    func testPerplexityClientParsesCitationsArray() throws {
        let body = """
        {"choices":[{"message":{"content":"hello"}}],"citations":["https://a","https://b"]}
        """
        let result = try PerplexityClient.parseResponse(Data(body.utf8))
        XCTAssertEqual(result.answer, "hello")
        XCTAssertEqual(result.sources.map(\.url), ["https://a", "https://b"])
    }

    func testPerplexityClientParsesSearchResultsArray() throws {
        let body = """
        {"choices":[{"message":{"content":"x"}}],"search_results":[{"title":"Wiki","url":"https://wiki"}]}
        """
        let result = try PerplexityClient.parseResponse(Data(body.utf8))
        XCTAssertEqual(result.sources.first?.title, "Wiki")
        XCTAssertEqual(result.sources.first?.url, "https://wiki")
    }

    func testPerplexityClientHandlesMissingChoices() throws {
        let body = "{}"
        let result = try PerplexityClient.parseResponse(Data(body.utf8))
        XCTAssertEqual(result.answer, "")
        XCTAssertTrue(result.sources.isEmpty)
    }

    // MARK: - publish_episode

    func testPublishEpisodeSuccessReturnsNaddr() async throws {
        let ownedPodcasts = MockOwnedPodcasts()
        let deps = makeDeps(ownedPodcasts: ownedPodcasts)
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.publishEpisode,
            args: ["episode_id": "ep-abc"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["success"] as? Bool, true)
        XCTAssertEqual(decoded["episode_id"] as? String, "ep-abc")
        XCTAssertNotNil(decoded["naddr"])
        let published = await ownedPodcasts.publishedEpisodeIDs
        XCTAssertEqual(published, ["ep-abc"])
    }

    func testPublishEpisodeRejectsMissingEpisodeID() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.publishEpisode,
            args: [:],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testPublishEpisodeReturnsErrorWhenNotPublished() async throws {
        let ownedPodcasts = MockOwnedPodcasts()
        await ownedPodcasts.setShouldFailPublish(true)
        let deps = makeDeps(ownedPodcasts: ownedPodcasts)
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.publishEpisode,
            args: ["episode_id": "ep-private"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testPublishEpisodeReturnsErrorOnThrow() async throws {
        let ownedPodcasts = MockOwnedPodcasts()
        await ownedPodcasts.setPublishError(AgentOwnedPodcastError.noSigningKey)
        let deps = makeDeps(ownedPodcasts: ownedPodcasts)
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.publishEpisode,
            args: ["episode_id": "ep-nosigning"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
        let errMsg = decoded["error"] as? String ?? ""
        XCTAssertTrue(errMsg.contains("signing key"), "Expected signing key error, got: \(errMsg)")
    }

    // MARK: - Helpers

    private func decode(_ json: String) throws -> [String: Any] {
        let raw = try JSONSerialization.jsonObject(with: Data(json.utf8))
        guard let obj = raw as? [String: Any] else {
            throw NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "non-object JSON"])
        }
        return obj
    }

    private struct DepsBundle {
        let bundle: PodcastAgentToolDeps
        let playback: MockPlayback
    }

    private func makeDeps(
        rag: PodcastAgentRAGSearchProtocol = MockRAG(),
        wiki: WikiStorageProtocol = MockWiki(),
        briefing: BriefingComposerProtocol = MockBriefing(),
        summarizer: EpisodeSummarizerProtocol = MockSummarizer(),
        fetcher: EpisodeFetcherProtocol = MockFetcher(),
        playback: MockPlayback = MockPlayback(),
        library: PodcastLibraryProtocol = MockLibrary(),
        inventory: PodcastInventoryProtocol = MockInventory(),
        categories: PodcastCategoryProtocol = MockInventory(),
        peerPublisher: PeerEventPublisherProtocol = MockPeerEventPublisher(),
        friendDirectory: FriendDirectoryProtocol = MockFriendDirectory(),
        perplexity: PerplexityClientProtocol = MockPerplexity(),
        ttsPublisher: TTSPublisherProtocol = MockTTSPublisher(),
        ownedPodcasts: AgentOwnedPodcastManagerProtocol = MockOwnedPodcasts()
    ) -> DepsBundle {
        DepsBundle(
            bundle: PodcastAgentToolDeps(
                rag: rag,
                wiki: wiki,
                briefing: briefing,
                summarizer: summarizer,
                fetcher: fetcher,
                playback: playback,
                library: library,
                inventory: inventory,
                categories: categories,
                peerPublisher: peerPublisher,
                friendDirectory: friendDirectory,
                perplexity: perplexity,
                ttsPublisher: ttsPublisher,
                directory: MockDirectory(),
                subscribe: MockSubscribe(),
                youtubeIngestion: MockYouTubeIngestion(),
                ownedPodcasts: ownedPodcasts
            ),
            playback: playback
        )
    }
}

// Mocks live in `AgentToolsPodcastMocks.swift` (split out to keep this file
// under the 500-line hard cap).
