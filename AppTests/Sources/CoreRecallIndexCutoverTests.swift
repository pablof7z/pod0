import CSQLiteVec
import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class CoreRecallIndexCutoverTests: XCTestCase {
    func testTypedCommandDeletesOnlyLegacyDisposableIndexArtifacts() async throws {
        let root = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let legacy = root.appendingPathComponent("vectors.sqlite")
        let wal = URL(fileURLWithPath: legacy.path + "-wal")
        let sharedMemory = URL(fileURLWithPath: legacy.path + "-shm")
        let unrelated = root.appendingPathComponent("vectors.sqlite.backup")
        try createPopulatedSQLite(at: legacy)
        try Data("legacy-wal".utf8).write(to: wal)
        try Data("legacy-shm".utf8).write(to: sharedMemory)
        try Data("keep".utf8).write(to: unrelated)

        let facade = Pod0Facade()
        let commandID = CommandId(uuid: UUID())
        let cancellationID = CancellationId(uuid: UUID())
        facade.dispatch(command: CommandEnvelope(
            commandId: commandID,
            cancellationId: cancellationID,
            expectedRevision: nil,
            command: .commitRecallIndexCutover
        ))
        let request = try XCTUnwrap(facade.nextHostRequests(maximumCount: 1).first)
        XCTAssertEqual(request.request, .removeLegacyRecallIndexArtifacts)

        let host = makeHost(legacyIndexURL: legacy)
        let observation = await host.execute(request.request)
        XCTAssertEqual(
            observation,
            .legacyRecallIndexArtifactsRemoved(removedFileCount: 3)
        )
        facade.recordHostObservation(observation: HostObservationEnvelope(
            requestId: request.requestId,
            cancellationId: request.cancellationId,
            observedRequestRevision: request.issuedRevision,
            sequenceNumber: 0,
            observedAt: UnixTimestampMilliseconds(value: 1_800_000_000_000),
            observation: observation
        ))

        let operation = try XCTUnwrap(operation(facade, commandID: commandID))
        XCTAssertEqual(operation.stage, .succeeded)
        XCTAssertEqual(
            operation.result,
            .recallIndexCutoverCommitted(schemaVersion: 1, removedLegacyFileCount: 3)
        )
        XCTAssertFalse(FileManager.default.fileExists(atPath: legacy.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: wal.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: sharedMemory.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: unrelated.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.path))
    }

    func testLegacyArtifactCapabilityFailsClosedForDirectory() async throws {
        let root = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let legacy = root.appendingPathComponent("vectors.sqlite", isDirectory: true)
        try FileManager.default.createDirectory(at: legacy, withIntermediateDirectories: false)

        let observation = await makeHost(legacyIndexURL: legacy)
            .execute(.removeLegacyRecallIndexArtifacts)

        guard case .failed(code: .invalidResponse, safeDetail: let detail) = observation else {
            return XCTFail("Expected a typed fail-closed observation")
        }
        XCTAssertEqual(detail, "Invalid legacy recall artifact")
        XCTAssertTrue(FileManager.default.fileExists(atPath: legacy.path))
    }

    func testLegacyArtifactCapabilityFailsClosedForSymbolicLink() async throws {
        let root = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let target = root.appendingPathComponent("unrelated.sqlite")
        let legacy = root.appendingPathComponent("vectors.sqlite")
        try Data("keep".utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: legacy, withDestinationURL: target)

        let observation = await makeHost(legacyIndexURL: legacy)
            .execute(.removeLegacyRecallIndexArtifacts)

        guard case .failed(code: .invalidResponse, safeDetail: _) = observation else {
            return XCTFail("Expected a typed fail-closed observation")
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: legacy.path))
        XCTAssertEqual(try Data(contentsOf: target), Data("keep".utf8))
    }

    func testLegacyArtifactCapabilityFailsClosedWithoutResolvedLocation() async {
        let observation = await makeHost(legacyIndexURL: nil)
            .execute(.removeLegacyRecallIndexArtifacts)

        guard case .failed(code: .platformFailure, safeDetail: let detail) = observation else {
            return XCTFail("Expected a typed location failure")
        }
        XCTAssertEqual(detail, "Legacy recall location unavailable")
    }

    private func makeHost(legacyIndexURL: URL?) -> CoreRecallHost {
        CoreRecallHost(
            embedder: UnusedCutoverEmbedder(),
            reranker: UnusedCutoverReranker(),
            legacyIndexURL: legacyIndexURL,
            isRerankingEnabled: { false }
        )
    }

    private func makeTemporaryDirectory() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-recall-cutover-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private func createPopulatedSQLite(at url: URL) throws {
        var database: OpaquePointer?
        XCTAssertEqual(sqlite3_open(url.path, &database), SQLITE_OK)
        let opened = try XCTUnwrap(database)
        defer { XCTAssertEqual(sqlite3_close(opened), SQLITE_OK) }
        XCTAssertEqual(
            sqlite3_exec(
                opened,
                "CREATE TABLE chunks_transcript(id TEXT PRIMARY KEY, embedding BLOB);"
                    + "INSERT INTO chunks_transcript VALUES('populated', X'01020304');",
                nil,
                nil,
                nil
            ),
            SQLITE_OK
        )
    }

    private func operation(
        _ facade: Pod0Facade,
        commandID: CommandId
    ) -> OperationProjection? {
        guard case .library(let library) = facade.snapshot(request: ProjectionRequest(
            scope: .library,
            offset: 0,
            maxItems: 20
        )).projection else { return nil }
        return library.operations.first { $0.commandId == commandID }
    }
}

private struct UnusedCutoverEmbedder: EmbeddingsClient {
    func embed(_ texts: [String]) async throws -> [[Float]] {
        throw EmbeddingsError.missingAPIKey
    }
}

private struct UnusedCutoverReranker: RerankerClient {
    func rerank(query: String, documents: [String], topN: Int?) async throws -> [Int] {
        throw RerankerError.missingAPIKey
    }
}
