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
            feedHost: DownloadVerticalFeedHost()
        ) else { return XCTFail("Expected shared core bootstrap") }
        let facade = bootstrap.facade
        bootstrap.shutdown()

        let stagedURL = fixture.fileURL.deletingLastPathComponent().appendingPathComponent(
            "native-download-\(UUID().uuidString).media"
        )
        let bytes = Data("end-to-end-native-download".utf8)
        try bytes.write(to: stagedURL)
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
        dispatcher.executePendingRequests(from: facade)
        try await waitUntil { dispatcher.observationRecoveryReady }

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
        try await waitUntil {
            self.workflow(facade, episodeID: fixture.episodeID)?.stage == .succeeded
        }

        XCTAssertEqual(host.executeCount, 1)
        let pendingOutboxCount = await outbox.pendingCount()
        XCTAssertEqual(pendingOutboxCount, 0)
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

    private func waitUntil(_ condition: @escaping @MainActor () -> Bool) async throws {
        for _ in 0 ..< 200 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Condition did not become true")
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
