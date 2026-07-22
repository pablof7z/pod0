import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class Pod0NativeHostDispatcherTests: XCTestCase {
    func testExpiredAndDuplicateRequestsNeverExecuteAnEffectTwice() async {
        let feed = RecordingCoreFeedHost()
        let playback = FakeCorePlaybackHost()
        let clock = Date(timeIntervalSince1970: 10)
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: feed,
            playbackHost: playback,
            now: { clock }
        )
        let expired = envelope(
            requestID: 1,
            deadline: Date(timeIntervalSince1970: 9),
            request: feedRequest
        )
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(expired) { observations.append($0) }
        dispatcher.execute(expired) { observations.append($0) }

        XCTAssertEqual(observations.count, 1)
        XCTAssertEqual(observations[0].requestId, expired.requestId)
        XCTAssertEqual(observations[0].cancellationId, expired.cancellationId)
        guard case .failed(code: .timedOut, safeDetail: _) = observations[0].observation else {
            return XCTFail("Expected typed deadline failure")
        }
        let feedCallCount = await feed.callCount
        XCTAssertEqual(feedCallCount, 0)
    }

    func testCancellationEmitsOnceAndSuppressesLateFeedCompletion() async {
        let feed = SuspendingCoreFeedHost()
        let playback = FakeCorePlaybackHost()
        let dispatcher = Pod0NativeHostDispatcher(feedHost: feed, playbackHost: playback)
        let request = envelope(requestID: 2, deadline: nil, request: feedRequest)
        var observations: [HostObservationEnvelope] = []

        dispatcher.execute(request) { observations.append($0) }
        dispatcher.cancel(cancellationID: request.cancellationId)
        await Task.yield()
        await Task.yield()

        XCTAssertEqual(observations.count, 1)
        XCTAssertEqual(observations[0].sequenceNumber, 0)
        XCTAssertEqual(observations[0].observation, .cancelled)
    }

    func testRecallRequestFailsTypedWhenCapabilityIsNotAttached() async {
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: RecordingCoreFeedHost(),
            playbackHost: FakeCorePlaybackHost()
        )
        let request = envelope(
            requestID: 4,
            deadline: nil,
            request: .embedRecallQuery(
                queryId: RecallQueryId(high: 1, low: 2),
                provider: .openRouter,
                model: "embedding-model",
                text: "Where was the memory model discussed?",
                maximumDimensions: 1_536
            )
        )
        var observations: [HostObservationEnvelope] = []
        let delivered = expectation(description: "typed recall failure delivered")

        dispatcher.execute(request) {
            observations.append($0)
            delivered.fulfill()
        }

        await fulfillment(of: [delivered], timeout: 1)
        guard let observation = observations.first?.observation,
              case .failed(code: .indexUnavailable, safeDetail: _) = observation else {
            return XCTFail("Expected typed recall capability failure")
        }
        XCTAssertEqual(observations.count, 1)
    }

    func testPublisherRequestUsesRustTimingAndDeliversRawObservation() async {
        let publisher = RecordingCorePublisherChapterHost()
        let clock = Date(timeIntervalSince1970: 100)
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: RecordingCoreFeedHost(),
            publisherChapterHost: publisher,
            playbackHost: FakeCorePlaybackHost(),
            now: { clock }
        )
        let request = envelope(
            requestID: 5,
            deadline: clock.addingTimeInterval(30),
            request: .fetchPublisherChapters(
                episodeId: EpisodeId(high: 3, low: 4),
                sourceUrl: "https://example.test/chapters.json",
                notBefore: UnixTimestampMilliseconds(date: clock.addingTimeInterval(-1)),
                maximumResponseBytes: 4_096
            )
        )
        var observations: [HostObservationEnvelope] = []
        let delivered = expectation(description: "publisher observation delivered")

        dispatcher.execute(request) {
            observations.append($0)
            delivered.fulfill()
        }
        await fulfillment(of: [delivered], timeout: 1)

        let publisherCallCount = await publisher.callCount
        XCTAssertEqual(publisherCallCount, 1)
        guard case .publisherChaptersFetched(_, let bytes, _, _, _, _, let status)
            = observations.first?.observation else {
            return XCTFail("Expected raw publisher observation")
        }
        XCTAssertEqual(bytes, Data("raw".utf8))
        XCTAssertEqual(status, 404)
    }

    func testPlaybackStreamCoalescesPositionButNeverDropsLifecycleBoundaries() {
        let feed = RecordingCoreFeedHost()
        let playback = FakeCorePlaybackHost()
        var clock = Date(timeIntervalSince1970: 100)
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: feed,
            playbackHost: playback,
            now: { clock }
        )
        let episodeID = EpisodeId(uuid: UUID())
        playback.observation = observation(episodeID: episodeID, state: .prepared, position: 0)
        let request = envelope(
            requestID: 3,
            deadline: nil,
            request: .observePlayback(
                episodeId: episodeID,
                minimumIntervalMilliseconds: 1_000
            )
        )
        var delivered: [HostObservationEnvelope] = []

        dispatcher.execute(request) { delivered.append($0) }
        clock = clock.addingTimeInterval(0.1)
        playback.emit(observation(episodeID: episodeID, state: .prepared, position: 100))
        playback.emit(observation(episodeID: episodeID, state: .playing, position: 100))
        clock = clock.addingTimeInterval(1.1)
        playback.emit(observation(episodeID: episodeID, state: .playing, position: 1_200))

        XCTAssertEqual(delivered.map(\.sequenceNumber), [1, 2, 3])
        XCTAssertEqual(delivered.map(\.observedRequestRevision), [request.issuedRevision, request.issuedRevision, request.issuedRevision])
        guard case .playbackObserved(let last) = delivered.last?.observation else {
            return XCTFail("Expected typed playback stream")
        }
        XCTAssertEqual(last.positionMilliseconds, 1_200)

        dispatcher.cancel(cancellationID: request.cancellationId)
        XCTAssertEqual(delivered.map(\.sequenceNumber), [1, 2, 3, 4])
        XCTAssertEqual(delivered.last?.observation, .cancelled)
    }

    private var feedRequest: HostRequest {
        .fetchFeed(
            feedUrl: "https://feeds.example.test/show.xml",
            entityTag: "\"v1\"",
            lastModified: nil,
            maximumResponseBytes: 1_024
        )
    }

    private func envelope(
        requestID: UInt64,
        deadline: Date?,
        request: HostRequest
    ) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 0, low: requestID),
            commandId: CommandId(high: 0, low: requestID),
            cancellationId: CancellationId(high: 9, low: requestID),
            issuedRevision: StateRevision(value: 7),
            deadlineAt: deadline.map(UnixTimestampMilliseconds.init(date:)),
            request: request
        )
    }

    private func observation(
        episodeID: EpisodeId,
        state: PlaybackHostState,
        position: UInt64
    ) -> PlaybackLifecycleObservation {
        PlaybackLifecycleObservation(
            episodeId: episodeID,
            state: state,
            positionMilliseconds: position,
            durationMilliseconds: 10_000,
            route: .builtIn,
            interruption: .none,
            ended: false
        )
    }
}

private actor RecordingCoreFeedHost: CoreFeedHosting {
    private(set) var callCount = 0

    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        callCount += 1
        return .feedBytesFetched(
            bytes: Data("feed".utf8),
            entityTag: nil,
            lastModified: nil,
            responseUrl: "https://feeds.example.test/show.xml",
            httpStatus: 200
        )
    }
}

private actor SuspendingCoreFeedHost: CoreFeedHosting {
    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        do {
            try await Task.sleep(for: .seconds(30))
            return .failed(code: .platformFailure, safeDetail: "Unexpected completion")
        } catch {
            return .cancelled
        }
    }
}

private actor RecordingCorePublisherChapterHost: CorePublisherChapterHosting {
    private(set) var callCount = 0

    func fetch(
        episodeID: EpisodeId,
        sourceURL _: String,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        callCount += 1
        return .publisherChaptersFetched(
            episodeId: episodeID,
            bytes: Data("raw".utf8),
            contentType: "application/json",
            responseUrl: "https://example.test/chapters.json",
            entityTag: nil,
            lastModified: nil,
            httpStatus: 404
        )
    }
}

@MainActor
private final class FakeCorePlaybackHost: CorePlaybackHosting {
    var observation = PlaybackLifecycleObservation(
        episodeId: nil,
        state: .idle,
        positionMilliseconds: 0,
        durationMilliseconds: 0,
        route: .unknown,
        interruption: .none,
        ended: false
    )
    private var sink: (PlaybackLifecycleObservation) -> Void = { _ in }

    func execute(_ request: HostRequest) -> HostObservation {
        switch request {
        case .observePlayback:
            .playbackObserved(value: observation)
        default:
            .failed(code: .invalidResponse, safeDetail: "Unexpected playback request")
        }
    }

    func installObservationSink(
        _ sink: @escaping (PlaybackLifecycleObservation) -> Void
    ) {
        self.sink = sink
    }

    func emit(_ observation: PlaybackLifecycleObservation) {
        self.observation = observation
        sink(observation)
    }
}
