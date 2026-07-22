import AVFoundation
import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreDownloadVerticalSliceTests: XCTestCase {
    func testTypedNativeObservationsAdoptArtifactAndSurviveFacadeRestart() async throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        guard case .ready(let bootstrap) = SharedLibraryBootstrap.run(
            persistence: fixture.persistence,
            legacyState: try fixture.persistence.load(),
            feedHost: DownloadVerticalFeedHost()
        ) else { return XCTFail("Expected shared core bootstrap") }
        let facade = bootstrap.facade
        bootstrap.shutdown()

        let stagedURL = fixture.fileURL.deletingLastPathComponent().appendingPathComponent(
            "native-download-\(UUID().uuidString).media"
        )
        try SilentAudioWriter.writeSilence(durationSeconds: 0.1, to: stagedURL)
        let bytes = try Data(contentsOf: stagedURL)
        defer { try? FileManager.default.removeItem(at: stagedURL) }
        let host = ImmediateDownloadHost(stagedURL: stagedURL, byteCount: UInt64(bytes.count))
        let outbox = try NativeHostObservationOutbox(
            fileURL: fixture.fileURL.appendingPathExtension("download-outbox.json")
        )
        let dispatcher = Pod0NativeHostDispatcher(
            feedHost: DownloadVerticalFeedHost(),
            downloadHost: host,
            playbackHost: DownloadVerticalPlaybackHost(),
            observationOutbox: outbox
        )
        dispatcher.activateExecution()
        let completed = expectation(description: "Rust persisted native download evidence")
        let subscriber = SuccessfulDownloadSubscriber(
            episodeID: fixture.episodeID,
            completed: completed
        )
        let subscription = facade.subscribe(
            request: ProjectionRequest(
                scope: .downloads(episodeId: EpisodeId(uuid: fixture.episodeID)),
                offset: 0,
                maxItems: 20
            ),
            subscriber: subscriber
        )
        defer { facade.unsubscribe(subscriptionId: subscription) }

        facade.dispatch(command: command(
            1,
            .observeDownloadEnvironment(observation: DownloadEnvironmentObservation(
                network: .wifi,
                availableCapacityBytes: 2_000_000_000
            ))
        ))
        facade.dispatch(command: command(
            2,
            .requestEpisodeDownload(
                episodeId: EpisodeId(uuid: fixture.episodeID),
                origin: .user
            )
        ))
        dispatcher.executePendingRequests(from: facade)
        await fulfillment(of: [completed], timeout: 3)

        XCTAssertEqual(host.executeCount, 1)
        let pendingObservationCount = await outbox.pendingCount()
        XCTAssertEqual(pendingObservationCount, 0)
        let completedWorkflow = workflow(facade, episodeID: fixture.episodeID)
        XCTAssertEqual(
            completedWorkflow?.stage,
            .succeeded,
            "Unexpected workflow after native evidence: \(String(describing: completedWorkflow))"
        )
        guard case let .episodeDetail(detail) = facade.snapshot(request: ProjectionRequest(
            scope: .episodeDetail(episodeId: EpisodeId(uuid: fixture.episodeID)),
            offset: 0,
            maxItems: 20
        )).projection else { return XCTFail("Expected episode detail") }
        guard case let .available(reference, byteCount) = detail.episode?.download else {
            return XCTFail("Expected Rust-owned available artifact")
        }
        XCTAssertEqual(byteCount, UInt64(bytes.count))
        let artifactRoot = fixture.persistence.sharedCoreStoreURL
            .deletingLastPathComponent()
            .appendingPathComponent(
                fixture.persistence.sharedCoreStoreURL.lastPathComponent + ".downloads",
                isDirectory: true
            )
        let artifactURL = artifactRoot.appendingPathComponent(reference.opaqueKey)
        XCTAssertEqual(try Data(contentsOf: artifactURL), bytes)

        dispatcher.shutdown()
        let reopened = try Pod0Facade.open(storePath: fixture.persistence.sharedCoreStoreURL.path)
        XCTAssertEqual(workflow(reopened, episodeID: fixture.episodeID)?.stage, .succeeded)
        guard case let .episodeDetail(reopenedDetail) = reopened.snapshot(
            request: ProjectionRequest(
                scope: .episodeDetail(episodeId: EpisodeId(uuid: fixture.episodeID)),
                offset: 0,
                maxItems: 20
            )
        ).projection else { return XCTFail("Expected reopened episode detail") }
        XCTAssertEqual(reopenedDetail.episode?.download, detail.episode?.download)
        let mappingClient = SharedLibraryClient(
            facade: reopened,
            coreStoreURL: fixture.persistence.sharedCoreStoreURL,
            feedHost: DownloadVerticalFeedHost()
        )
        let mappedState = mappingClient.downloadState(
            for: try XCTUnwrap(reopenedDetail.episode?.download)
        )
        XCTAssertEqual(
            mappedState,
            .downloaded(localFileURL: artifactURL, byteCount: Int64(bytes.count))
        )
        let offlineEpisode = Episode(
            id: fixture.episodeID,
            podcastID: fixture.podcastID,
            guid: "offline-download-smoke",
            title: "Offline Download Smoke",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://offline.example/episode.m4a")!,
            downloadState: mappedState
        )
        let engine = AudioEngine()
        let playbackHost = CorePlaybackHost(engine: engine) { id in
            id == offlineEpisode.id ? offlineEpisode : nil
        }
        let episodeID = EpisodeId(uuid: offlineEpisode.id)
        guard case .playbackObserved = playbackHost.execute(.loadMedia(
            episodeId: episodeID,
            audioUrl: offlineEpisode.enclosureURL.absoluteString,
            startPositionMilliseconds: 0
        )) else { return XCTFail("Expected native host to load verified local audio") }
        let loadedAsset = try XCTUnwrap(engine.player.currentItem?.asset as? AVURLAsset)
        XCTAssertEqual(loadedAsset.url.standardizedFileURL, artifactURL.standardizedFileURL)
        guard case .playbackObserved(let playObservation) = playbackHost.execute(.play(
            episodeId: episodeID,
            transitionCue: .immediate
        )) else { return XCTFail("Expected native host to play verified local audio") }
        XCTAssertEqual(playObservation.state, .playing)
        _ = playbackHost.execute(.pause(episodeId: episodeID))
        mappingClient.shutdown()
    }

    private func command(_ low: UInt64, _ command: ApplicationCommand) -> CommandEnvelope {
        CommandEnvelope(
            commandId: CommandId(high: 30, low: low),
            cancellationId: CancellationId(high: 31, low: low),
            expectedRevision: nil,
            command: command
        )
    }

    private func workflow(_ facade: Pod0Facade, episodeID: UUID) -> DownloadWorkflowProjection? {
        guard case let .downloads(projection) = facade.snapshot(request: ProjectionRequest(
            scope: .downloads(episodeId: EpisodeId(uuid: episodeID)),
            offset: 0,
            maxItems: 20
        )).projection else { return nil }
        return projection.workflows.first
    }

}

private final class SuccessfulDownloadSubscriber: ProjectionSubscriber, @unchecked Sendable {
    private let episodeID: UUID
    private let completed: XCTestExpectation
    private let lock = NSLock()
    private var fulfilled = false

    init(episodeID: UUID, completed: XCTestExpectation) {
        self.episodeID = episodeID
        self.completed = completed
    }

    func receive(projection: ProjectionEnvelope) {
        guard case .downloads(let value) = projection.projection,
              value.workflows.contains(where: {
                  $0.episodeId.uuid == episodeID && $0.stage == .succeeded
              }) else { return }
        lock.withLock {
            guard !fulfilled else { return }
            fulfilled = true
            completed.fulfill()
        }
    }
}

@MainActor
private final class ImmediateDownloadHost: CoreDownloadHosting {
    let stagedURL: URL
    let byteCount: UInt64
    private(set) var executeCount = 0

    init(stagedURL: URL, byteCount: UInt64) {
        self.stagedURL = stagedURL
        self.byteCount = byteCount
    }

    func installOrphanObservationSink(_: @escaping OrphanDelivery) {}

    func execute(_ envelope: HostRequestEnvelope, delivery: @escaping Delivery) {
        executeCount += 1
        guard case let .startEpisodeDownload(episodeID, intentID, attemptID, _, _, _) =
            envelope.request else {
            delivery(1, .failed(code: .invalidResponse, safeDetail: nil))
            return
        }
        delivery(1, .downloadAccepted(
            episodeId: episodeID,
            intentId: intentID,
            attemptId: attemptID,
            externalTaskKey: "test-task",
            resumeKey: "v1/test.resume"
        ))
        delivery(2, .downloadStaged(
            episodeId: episodeID,
            intentId: intentID,
            attemptId: attemptID,
            stagedFilePath: stagedURL.path,
            byteCount: byteCount
        ))
    }

    func cancel(requestID _: HostRequestId, cancellationID _: CancellationId) {}
    func retire(
        requestID _: HostRequestId,
        observation _: HostObservation,
        receipt _: HostObservationReceipt
    ) {}
    func shutdown() {}
}

private struct DownloadVerticalFeedHost: CoreFeedHosting {
    func fetch(
        feedURL _: String,
        entityTag _: String?,
        lastModified _: String?,
        maximumResponseBytes _: UInt64,
        deadline _: Date?
    ) async -> HostObservation {
        .failed(code: .platformFailure, safeDetail: nil)
    }
}

@MainActor
private final class DownloadVerticalPlaybackHost: CorePlaybackHosting {
    func execute(_: HostRequest) -> HostObservation {
        .failed(code: .invalidResponse, safeDetail: nil)
    }
    func installObservationSink(_: @escaping (PlaybackLifecycleObservation) -> Void) {}
}
