import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class Pod0NativeModelWorkflowIntegrationTests: XCTestCase {
    func testNativeDispatcherPersistsCompletionAndRepeatedEnsureNeverReposts() async throws {
        let fixture = try makeFixture()
        defer { dispose(fixture) }
        let outbox = try NativeHostObservationOutbox(fileURL: fixture.outboxURL)
        let modelHost = SuccessfulNativeModelHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: QueuedCoreFeedHost([]),
            chapterModelHost: modelHost,
            playbackHost: NativeModelPlaybackHost(),
            observationOutbox: outbox
        )

        dispatcher.activateExecution()
        ensure(fixture.facade, episodeID: fixture.episodeID, commandLow: 1)
        dispatcher.executePendingRequests(from: fixture.facade)

        try await waitForStage(.succeeded, facade: fixture.facade, episodeID: fixture.episodeID)
        let firstCallCount = await modelHost.callCount()
        let firstPendingCount = await outbox.pendingCount()
        XCTAssertEqual(firstCallCount, 1)
        XCTAssertEqual(firstPendingCount, 0)
        XCTAssertTrue(fixture.facade.nextHostRequests(maximumCount: 64).isEmpty)

        ensure(fixture.facade, episodeID: fixture.episodeID, commandLow: 2)
        dispatcher.executePendingRequests(from: fixture.facade)
        try await Task.sleep(for: .milliseconds(50))
        let repeatedCallCount = await modelHost.callCount()
        XCTAssertEqual(repeatedCallCount, 1)
        XCTAssertTrue(fixture.facade.nextHostRequests(maximumCount: 64).isEmpty)
        dispatcher.shutdown()
    }

    func testRelaunchReplaysDurableCompletionBeforeAnyRecoveryRequest() async throws {
        let fixture = try makeFixture()
        defer { dispose(fixture) }
        ensure(fixture.facade, episodeID: fixture.episodeID, commandLow: 3)
        let request = try XCTUnwrap(modelRequest(in: fixture.facade.nextHostRequests(
            maximumCount: 64
        )))
        let outbox = try NativeHostObservationOutbox(fileURL: fixture.outboxURL)
        try await outbox.persistBeforeDelivery(completionEnvelope(request))

        let reopened = try Pod0Facade.open(storePath: fixture.coreStoreURL.path)
        let relaunchedOutbox = try NativeHostObservationOutbox(fileURL: fixture.outboxURL)
        let modelHost = SuccessfulNativeModelHost()
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: QueuedCoreFeedHost([]),
            chapterModelHost: modelHost,
            playbackHost: NativeModelPlaybackHost(),
            observationOutbox: relaunchedOutbox
        )

        dispatcher.activateExecution()
        dispatcher.executePendingRequests(from: reopened)

        try await waitForStage(.succeeded, facade: reopened, episodeID: fixture.episodeID)
        let recoveryCallCount = await modelHost.callCount()
        let recoveryPendingCount = await relaunchedOutbox.pendingCount()
        XCTAssertEqual(recoveryCallCount, 0)
        XCTAssertEqual(recoveryPendingCount, 0)
        XCTAssertTrue(reopened.nextHostRequests(maximumCount: 64).isEmpty)
        dispatcher.shutdown()
    }

    private struct Fixture {
        let store: AppStateStore
        let fileURL: URL
        let coreStoreURL: URL
        let outboxURL: URL
        let facade: Pod0Facade
        let podcastID: UUID
        let episodeID: UUID
    }

    private func makeFixture() throws -> Fixture {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        let podcast = Podcast(
            id: UUID(),
            feedURL: URL(string: "https://model.example.test/feed.xml")!,
            title: "Model Workflow"
        )
        let episode = Episode(
            id: UUID(),
            podcastID: podcast.id,
            guid: "model-workflow",
            title: "Durable Model Workflow",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            duration: 120,
            enclosureURL: URL(string: "https://model.example.test/episode.mp3")!
        )
        var state = AppState()
        state.settings.chapterCompilationModel = "ollama:llama3.2"
        state.podcasts = [podcast]
        state.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
        state.episodes = [episode]
        XCTAssertTrue(persistence.write(state, revision: 1))
        let store = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        let client = try XCTUnwrap(store.sharedLibrary)
        _ = try client.submitTranscriptObservation(
            Transcript(
                episodeID: episode.id,
                language: "en-US",
                source: .publisher,
                segments: [
                    Segment(start: 0, end: 30, text: "Opening context"),
                    Segment(start: 30, end: 60, text: "A deeper idea"),
                    Segment(start: 60, end: 90, text: "Evidence and implications"),
                    Segment(start: 90, end: 120, text: "Closing thought"),
                ]
            ),
            context: TranscriptObservationContext(
                podcastID: podcast.id,
                sourceRevision: "audio-v1",
                sourcePayloadDigest: ArtifactRepository.hash(Data("audio-v1".utf8)),
                provider: "publisher"
            )
        )
        let facade = client.facade
        client.shutdown()
        return Fixture(
            store: store,
            fileURL: fileURL,
            coreStoreURL: persistence.sharedCoreStoreURL,
            outboxURL: fileURL.appendingPathExtension("model-outbox.json"),
            facade: facade,
            podcastID: podcast.id,
            episodeID: episode.id
        )
    }

    private func dispose(_ fixture: Fixture) {
        fixture.store.sharedLibrary?.shutdown()
        try? FileManager.default.removeItem(at: fixture.outboxURL)
        AppStateTestSupport.disposeIsolatedStore(at: fixture.fileURL)
    }

    private func ensure(_ facade: Pod0Facade, episodeID: UUID, commandLow: UInt64) {
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(high: 90, low: commandLow),
            cancellationId: CancellationId(high: 91, low: commandLow),
            expectedRevision: nil,
            command: .ensureModelChapters(
                episodeId: EpisodeId(uuid: episodeID),
                configuredModel: "ollama:llama3.2"
            )
        ))
    }

    private func modelRequest(in requests: [HostRequestEnvelope]) -> HostRequestEnvelope? {
        requests.first {
            if case .executeChapterModel = $0.request { true } else { false }
        }
    }

    private func completionEnvelope(_ request: HostRequestEnvelope) -> HostObservationEnvelope {
        HostObservationEnvelope(
            requestId: request.requestId,
            cancellationId: request.cancellationId,
            observedRequestRevision: request.issuedRevision,
            sequenceNumber: 1,
            observedAt: UnixTimestampMilliseconds(date: Date()),
            observation: SuccessfulNativeModelHost.completion(for: request.request)
        )
    }

    private func waitForStage(
        _ expected: ModelChapterWorkflowStage,
        facade: Pod0Facade,
        episodeID: UUID
    ) async throws {
        var observed: ModelChapterWorkflowProjection?
        for _ in 0 ..< 200 {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .chapterWorkflows(episodeId: EpisodeId(uuid: episodeID)),
                offset: 0,
                maxItems: 2
            ))
            if case .chapterWorkflows(let value) = envelope.projection {
                observed = value.model.first
                if observed?.stage == expected { return }
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail(
            "Model workflow did not reach \(expected); stage=\(String(describing: observed?.stage)) "
                + "failure=\(String(describing: observed?.failure))"
        )
    }
}

private actor SuccessfulNativeModelHost: CoreChapterModelHosting {
    private var calls = 0

    func execute(_ request: HostRequest) async -> HostObservation {
        calls += 1
        return Self.completion(for: request)
    }

    func callCount() -> Int { calls }

    static func completion(for request: HostRequest) -> HostObservation {
        guard case .executeChapterModel(
            let episodeID,
            let generation,
            let fence,
            _
        ) = request else {
            return .failed(code: .invalidResponse, safeDetail: "Unexpected model request")
        }
        return .chapterModelCompleted(
            episodeId: episodeID,
            generation: generation,
            submissionFenceId: fence,
            completion: ChapterModelCompletionObservation(
                completion: #"{"chapters":[{"start":0,"title":"Opening"},{"start":30,"title":"Context"},{"start":60,"title":"Deep dive"},{"start":90,"title":"Close"}],"ads":[]}"#,
                provider: "ollama",
                model: "llama3.2:latest",
                promptTokens: 100,
                completionTokens: 50,
                cachedTokens: 0,
                reasoningTokens: 0,
                costMicrousd: nil,
                providerOperationId: nil,
                providerStatus: "completed",
                providerGeneratedAt: nil
            )
        )
    }
}

@MainActor
private final class NativeModelPlaybackHost: CorePlaybackHosting {
    func execute(_: HostRequest) -> HostObservation {
        .failed(code: .invalidResponse, safeDetail: "Unexpected playback request")
    }

    func installObservationSink(_: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
