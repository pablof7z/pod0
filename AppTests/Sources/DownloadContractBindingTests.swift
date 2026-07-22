import Pod0Core
import XCTest

final class DownloadContractBindingTests: XCTestCase {
    func testDownloadCommandsAndHostPayloadsRemainTyped() {
        let episodeID = EpisodeId(high: 7, low: 8)
        let intentID = DownloadIntentId(high: 9, low: 10)
        let attemptID = DownloadAttemptId(high: 11, low: 12)
        let command = ApplicationCommand.requestEpisodeDownload(
            episodeId: episodeID,
            origin: .user
        )
        guard case let .requestEpisodeDownload(valueEpisodeID, origin) = command else {
            return XCTFail("Expected a typed download command")
        }
        XCTAssertEqual(valueEpisodeID, episodeID)
        XCTAssertEqual(origin, .user)

        let request = HostRequest.startEpisodeDownload(
            episodeId: episodeID,
            intentId: intentID,
            attemptId: attemptID,
            inputVersion: String(repeating: "a", count: 64),
            enclosureUrl: "https://example.test/audio.mp3",
            resumeKey: nil
        )
        guard case let .startEpisodeDownload(
            requestEpisodeID,
            requestIntentID,
            requestAttemptID,
            _,
            enclosureURL,
            _
        ) = request else {
            return XCTFail("Expected a typed start request")
        }
        XCTAssertEqual(requestEpisodeID, episodeID)
        XCTAssertEqual(requestIntentID, intentID)
        XCTAssertEqual(requestAttemptID, attemptID)
        XCTAssertEqual(enclosureURL, "https://example.test/audio.mp3")

        let observation = HostObservation.downloadStaged(
            episodeId: episodeID,
            intentId: intentID,
            attemptId: attemptID,
            stagedFilePath: "/tmp/download-12",
            byteCount: 4_096
        )
        guard case let .downloadStaged(_, _, observedAttemptID, _, byteCount) = observation else {
            return XCTFail("Expected a typed staged observation")
        }
        XCTAssertEqual(observedAttemptID, attemptID)
        XCTAssertEqual(byteCount, 4_096)
    }

    func testDownloadProjectionReportsUnavailableUntilDurableStoreLands() {
        let facade = Pod0Facade()
        facade.dispatch(
            command: CommandEnvelope(
                commandId: CommandId(high: 0, low: 201),
                cancellationId: CancellationId(high: 0, low: 202),
                expectedRevision: nil,
                command: .requestEpisodeDownload(
                    episodeId: EpisodeId(high: 7, low: 8),
                    origin: .user
                )
            )
        )
        let projection = facade.snapshot(
            request: ProjectionRequest(
                scope: .downloads(episodeId: nil),
                offset: 0,
                maxItems: 20
            )
        )

        XCTAssertEqual(projection.contractVersion, 35)
        guard case let .downloads(value) = projection.projection else {
            return XCTFail("Expected a download projection")
        }
        XCTAssertTrue(value.workflows.isEmpty)
        XCTAssertEqual(value.failure?.code, .storageUnavailable)
    }
}
