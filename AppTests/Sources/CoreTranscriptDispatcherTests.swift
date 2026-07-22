import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreTranscriptDispatcherTests: XCTestCase {
    func testTranscriptCapabilityRequiresDurableObservationStaging() {
        let host = CountingTranscriptHost(observation: .transcriptCapabilityObserved(
            observation: .providerPending(
                providerStatus: "processing",
                retryAfterMilliseconds: nil
            )
        ))
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: TranscriptDispatcherFeedHost(),
            playbackHost: TranscriptDispatcherPlaybackHost(),
            transcriptHost: host
        )
        var delivered: [HostObservationEnvelope] = []

        dispatcher.execute(envelope(id: 1)) { delivered.append($0) }

        XCTAssertEqual(delivered.count, 1)
        XCTAssertEqual(delivered[0].observation, .failed(
            code: .platformFailure,
            safeDetail: "Durable transcript observation staging is unavailable"
        ))
    }

    func testDuplicateStartExecutesExactlyOnce() async throws {
        let outbox = try makeOutbox()
        let host = CountingTranscriptHost(observation: .transcriptCapabilityObserved(
            observation: .providerPending(
                providerStatus: "processing",
                retryAfterMilliseconds: nil
            )
        ))
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: TranscriptDispatcherFeedHost(),
            playbackHost: TranscriptDispatcherPlaybackHost(),
            transcriptHost: host,
            observationOutbox: outbox
        )
        let request = envelope(id: 2)
        let delivered = expectation(description: "transcript observation delivered")
        delivered.expectedFulfillmentCount = 1

        dispatcher.execute(request) { _ in delivered.fulfill() }
        dispatcher.execute(request) { _ in delivered.fulfill() }
        await fulfillment(of: [delivered], timeout: 1)

        let startCount = await host.callCount()
        XCTAssertEqual(startCount, 1)
    }

    func testCancellationSuppressesLateTranscriptCallback() async throws {
        let outbox = try makeOutbox()
        let host = SuspendingTranscriptHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: TranscriptDispatcherFeedHost(),
            playbackHost: TranscriptDispatcherPlaybackHost(),
            transcriptHost: host,
            observationOutbox: outbox
        )
        let request = envelope(id: 3)
        var delivered: [HostObservationEnvelope] = []

        dispatcher.execute(request) { delivered.append($0) }
        dispatcher.cancel(
            requestID: request.requestId,
            cancellationID: request.cancellationId
        )
        await Task.yield()
        await Task.yield()

        XCTAssertTrue(delivered.isEmpty)
        let cancellationCount = await host.callCount()
        XCTAssertEqual(cancellationCount, 1)
    }

    private func envelope(id: UInt64) -> HostRequestEnvelope {
        HostRequestEnvelope(
            requestId: HostRequestId(high: 10, low: id),
            commandId: CommandId(high: 11, low: id),
            cancellationId: CancellationId(high: 12, low: id),
            issuedRevision: StateRevision(value: 13),
            deadlineAt: nil,
            request: .executeTranscriptCapability(capability: .recoverProvider(
                context: TranscriptCapabilityContext(
                    episodeId: EpisodeId(high: 14, low: id),
                    podcastId: PodcastId(high: 15, low: id),
                    sourceRevision: "audio-v1"
                ),
                attemptId: TranscriptAttemptId(high: 16, low: id),
                submissionFenceId: TranscriptSubmissionFenceId(high: 17, low: id),
                provider: .assemblyAi,
                model: "universal-3-pro",
                externalOperationId: "operation-\(id)",
                providerStatus: "processing",
                maximumResponseBytes: 1_000_000
            ))
        )
    }

    private func makeOutbox() throws -> NativeHostObservationOutbox {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("transcript-dispatcher-\(UUID().uuidString)")
            .appendingPathComponent("outbox.json")
        return try NativeHostObservationOutbox(fileURL: url)
    }
}

private actor CountingTranscriptHost: CoreTranscriptHosting {
    private var calls = 0
    private let observation: HostObservation

    init(observation: HostObservation) {
        self.observation = observation
    }

    func execute(_: HostRequest) async -> HostObservation {
        calls += 1
        return observation
    }

    func callCount() -> Int { calls }
}

private actor SuspendingTranscriptHost: CoreTranscriptHosting {
    private var calls = 0

    func execute(_: HostRequest) async -> HostObservation {
        calls += 1
        do {
            try await Task.sleep(for: .seconds(30))
        } catch {}
        return .transcriptCapabilityObserved(observation: .providerPending(
            providerStatus: "late",
            retryAfterMilliseconds: nil
        ))
    }

    func callCount() -> Int { calls }
}

private actor TranscriptDispatcherFeedHost: CoreFeedHosting {
    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        .failed(code: .invalidResponse, safeDetail: "Unexpected feed request")
    }
}

@MainActor
private final class TranscriptDispatcherPlaybackHost: CorePlaybackHosting {
    func execute(_: HostRequest) -> HostObservation {
        .failed(code: .invalidResponse, safeDetail: "Unexpected playback request")
    }

    func installObservationSink(_: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
