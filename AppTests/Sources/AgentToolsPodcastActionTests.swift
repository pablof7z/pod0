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

    // MARK: - Helpers

    private struct DepsBundle {
        let bundle: PodcastAgentToolDeps
        let playback: MockPlayback
        let library: MockLibrary
    }

    private func makeDeps(
        fetcher: EpisodeFetcherProtocol = MockFetcher()
    ) -> DepsBundle {
        let playback = MockPlayback()
        let library = MockLibrary()
        return DepsBundle(
            bundle: PodcastAgentToolDeps(
                knowledge: MockKnowledgeSearch(),
                summarizer: MockSummarizer(),
                fetcher: fetcher,
                playback: playback,
                library: library,
                inventory: MockInventory(),
                categories: MockInventory(),
                perplexity: MockPerplexity(),
                ttsPublisher: MockTTSPublisher(),
                directory: MockDirectory(),
                subscribe: MockSubscribe(),
                youtubeIngestion: MockYouTubeIngestion(),
                ownedPodcasts: MockOwnedPodcasts()
            ),
            playback: playback,
            library: library
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
