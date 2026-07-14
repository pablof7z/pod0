import XCTest
@testable import Podcastr

@MainActor
final class AgentToolsPodcastActionTests: XCTestCase {

    func testPausePlaybackForwardsToPlaybackHost() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.pausePlayback,
            args: [:],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["state"] as? String, "paused")
        let pauseCount = await deps.playback.pauseCount
        XCTAssertEqual(pauseCount, 1)
    }

    func testSetPlaybackRateForwardsAndReturnsClampedRate() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.setPlaybackRate,
            args: ["rate": 4.2],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["requested_rate"] as? Double, 4.2)
        XCTAssertEqual(decoded["rate"] as? Double, 3.0)
        let rates = await deps.playback.recordedRates
        XCTAssertEqual(rates, [4.2])
    }

    func testSetSleepTimerRequiresMinutesForMinutesMode() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.setSleepTimer,
            args: ["mode": "minutes"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testSetSleepTimerForwardsEndOfEpisode() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.setSleepTimer,
            args: ["mode": "end_of_episode"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["mode"] as? String, "end_of_episode")
        let calls = await deps.playback.recordedSleepTimers
        XCTAssertEqual(calls.count, 1)
        XCTAssertEqual(calls.first?.0, "end_of_episode")
    }

    func testMarkEpisodePlayedMutatesLibrary() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.markEpisodePlayed,
            args: ["episode_id": "ep1"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["state"] as? String, "played")
        let playedIDs = await deps.library.playedIDs
        XCTAssertEqual(playedIDs, ["ep1"])
    }

    func testMarkEpisodeUnplayedMutatesLibrary() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.markEpisodeUnplayed,
            args: ["episode_id": "ep1"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["state"] as? String, "unplayed")
        let unplayedIDs = await deps.library.unplayedIDs
        XCTAssertEqual(unplayedIDs, ["ep1"])
    }

    func testDownloadEpisodeMutatesLibrary() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.downloadEpisode,
            args: ["episode_id": "ep1"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["state"] as? String, "downloading")
        let downloadedIDs = await deps.library.downloadedIDs
        XCTAssertEqual(downloadedIDs, ["ep1"])
    }

    func testRequestTranscriptionStartsIngest() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.requestTranscription,
            args: ["episode_id": "ep1"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["status"] as? String, "queued")
        let transcriptionIDs = await deps.library.transcriptionIDs
        XCTAssertEqual(transcriptionIDs, ["ep1"])
    }

    func testRefreshFeedForwardsPodcastID() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.refreshFeed,
            args: ["podcast_id": "pod1"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["title"] as? String, "Mock Show")
        XCTAssertEqual(decoded["new_episode_count"] as? Int, 2)
        let refreshedPodcastIDs = await deps.library.refreshedPodcastIDs
        XCTAssertEqual(refreshedPodcastIDs, ["pod1"])
    }

    func testCreateClipForwardsToLibrary() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.createClip,
            args: ["episode_id": "ep1", "start_seconds": 30.0, "end_seconds": 90.0, "caption": "Great moment"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["clip_id"] as? String, "mock-clip-1")
        XCTAssertEqual(decoded["episode_id"] as? String, "ep1")
        XCTAssertEqual(decoded["start_seconds"] as? Double, 30.0)
        XCTAssertEqual(decoded["end_seconds"] as? Double, 90.0)
        XCTAssertEqual(decoded["duration_seconds"] as? Double, 60.0)
        XCTAssertEqual(decoded["caption"] as? String, "Great moment")
        let calls = await deps.library.clipCalls
        XCTAssertEqual(calls.count, 1)
        XCTAssertEqual(calls.first?.episodeID, "ep1")
        XCTAssertEqual(calls.first?.caption, "Great moment")
    }

    func testCreateClipRejectsUnknownEpisode() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: []))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.createClip,
            args: ["episode_id": "unknown", "start_seconds": 0.0, "end_seconds": 30.0],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testCreateClipRejectsInvertedRange() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.createClip,
            args: ["episode_id": "ep1", "start_seconds": 90.0, "end_seconds": 30.0],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    func testCreateClipPassesTranscriptTextToLibrary() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.createClip,
            args: ["episode_id": "ep1", "start_seconds": 10.0, "end_seconds": 40.0,
                   "transcript_text": "Exactly what was said."],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["transcript_text"] as? String, "Exactly what was said.")
        let calls = await deps.library.clipCalls
        XCTAssertEqual(calls.first?.transcriptText, "Exactly what was said.")
    }

    func testDownloadAndTranscribeReturnsReadyStatus() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: ["ep1"]))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.downloadAndTranscribe,
            args: ["episode_id": "ep1"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["status"] as? String, "ready")
        XCTAssertEqual(decoded["source"] as? String, "mock")
        let transcriptionIDs = await deps.library.transcriptionIDs
        XCTAssertEqual(transcriptionIDs, ["ep1"])
        let downloadedIDs = await deps.library.downloadedIDs
        XCTAssertEqual(downloadedIDs, ["ep1"])
    }

    func testDownloadAndTranscribeRejectsUnknownEpisode() async throws {
        let deps = makeDeps(fetcher: MockFetcher(known: []))
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.downloadAndTranscribe,
            args: ["episode_id": "unknown"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNotNil(decoded["error"])
    }

    // MARK: - end_conversation

    func testEndConversationRequiresPeerContext() async throws {
        let deps = makeDeps()
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.endConversation,
            args: ["reason": "Nothing left to say"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(
            decoded["error"] as? String,
            "end_conversation requires a peer conversation context"
        )
    }

    func testEndConversationSignalsNoReplyWithoutMarkingEnded() async throws {
        let peerContext = PeerConversationContext(
            rootEventID: "root-event-1",
            inboundEventID: "inbound-event-1",
            peerPubkeyHex: "deadbeef"
        )
        let deps = makeDeps(peerContext: peerContext)
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.endConversation,
            args: ["reason": "Peer sent a bare acknowledgment"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["no_reply"] as? Bool, true)
        XCTAssertEqual(decoded["root_event_id"] as? String, "root-event-1")
        XCTAssertNil(decoded["error"])
        // Must NOT publish anything.
        let replies = await deps.peerPublisher.conversationReplies
        XCTAssertTrue(replies.isEmpty, "end_conversation must publish no outbound event")
    }

    // MARK: - send_friend_message

    func testSendFriendMessageWorksFromOwnerChat() async throws {
        // No peer context — simulates the user asking the agent to message someone
        // from regular in-app chat. Should publish a standalone kind:1 note.
        let deps = makeDeps(knownFriends: ["friend-pubkey-hex"])
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.sendFriendMessage,
            args: ["friend_pubkey": "friend-pubkey-hex", "message": "hey"],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertNil(decoded["error"])
        XCTAssertEqual(decoded["friend_pubkey"] as? String, "friend-pubkey-hex")
        XCTAssertNil(decoded["root_event_id"], "no root_event_id when there is no peer context")
        let calls = await deps.peerPublisher.friendMessages
        XCTAssertEqual(calls.count, 1)
        XCTAssertNil(calls.first?.peerContext, "standalone message should have no peer context")
    }

    func testSendFriendMessageRejectsUnknownPubkey() async throws {
        let peerContext = PeerConversationContext(
            rootEventID: "root-x",
            inboundEventID: "root-x",
            peerPubkeyHex: "peer-key"
        )
        let deps = makeDeps(peerContext: peerContext, knownFriends: ["alice-pubkey-hex"])
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.sendFriendMessage,
            args: [
                "friend_pubkey": "stranger-pubkey-hex",
                "message": "Hi there"
            ],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(
            decoded["error"] as? String,
            "No friend found matching 'stranger-pubkey-hex'. Add them first."
        )
        let calls = await deps.peerPublisher.friendMessages
        XCTAssertEqual(calls.count, 0)
    }

    func testSendFriendMessagePublishesWithPeerContext() async throws {
        let peerContext = PeerConversationContext(
            rootEventID: "root-3",
            inboundEventID: "root-3",
            peerPubkeyHex: "peer-key"
        )
        let deps = makeDeps(peerContext: peerContext, knownFriends: ["friend-pubkey-hex"])
        let json = await AgentTools.dispatchPodcast(
            name: AgentTools.PodcastNames.sendFriendMessage,
            args: [
                "friend_pubkey": "friend-pubkey-hex",
                "message": "Heads up — Alice wants you in the loop."
            ],
            deps: deps.bundle
        )
        let decoded = try decode(json)
        XCTAssertEqual(decoded["event_id"] as? String, "friend-msg-1")
        XCTAssertEqual(decoded["friend_pubkey"] as? String, "friend-pubkey-hex")
        XCTAssertEqual(decoded["root_event_id"] as? String, "root-3")
        let calls = await deps.peerPublisher.friendMessages
        XCTAssertEqual(calls.count, 1)
        XCTAssertEqual(calls.first?.friendPubkeyHex, "friend-pubkey-hex")
        XCTAssertEqual(calls.first?.peerContext?.rootEventID, "root-3")
    }

    // MARK: - Helpers

    private struct DepsBundle {
        let bundle: PodcastAgentToolDeps
        let playback: MockPlayback
        let library: MockLibrary
        let peerPublisher: MockPeerEventPublisher
    }

    private func makeDeps(
        fetcher: EpisodeFetcherProtocol = MockFetcher(),
        peerContext: PeerConversationContext? = nil,
        knownFriends: [String] = []
    ) -> DepsBundle {
        let playback = MockPlayback()
        let library = MockLibrary()
        let peerPublisher = MockPeerEventPublisher()
        return DepsBundle(
            bundle: PodcastAgentToolDeps(
                rag: MockRAG(),
                wiki: MockWiki(),
                briefing: MockBriefing(),
                summarizer: MockSummarizer(),
                fetcher: fetcher,
                playback: playback,
                library: library,
                inventory: MockInventory(),
                categories: MockInventory(),
                peerPublisher: peerPublisher,
                friendDirectory: MockFriendDirectory(knownPubkeys: knownFriends),
                perplexity: MockPerplexity(),
                ttsPublisher: MockTTSPublisher(),
                directory: MockDirectory(),
                subscribe: MockSubscribe(),
                youtubeIngestion: MockYouTubeIngestion(),
                ownedPodcasts: MockOwnedPodcasts(),
                peerContext: peerContext
            ),
            playback: playback,
            library: library,
            peerPublisher: peerPublisher
        )
    }

    private func decode(_ json: String) throws -> [String: Any] {
        let raw = try JSONSerialization.jsonObject(with: Data(json.utf8))
        guard let obj = raw as? [String: Any] else {
            throw NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "non-object JSON"])
        }
        return obj
    }
}
