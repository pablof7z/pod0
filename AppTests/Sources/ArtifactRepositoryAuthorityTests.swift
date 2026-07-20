import Foundation
import XCTest
import os
@testable import Podcastr

final class ArtifactRepositoryAuthorityTests: XCTestCase {
    func testAggregateReadExcludesEverySharedCoreOwnedArtifactKind() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "artifact-read-authority-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let repository = ArtifactRepository(fileURL: directory.appendingPathComponent("jobs.sqlite"))
        let nativeSubjectID = UUID()
        try repository.adopt(ArtifactRecord(
            kind: .semanticIndex,
            subjectID: nativeSubjectID,
            inputVersion: "native-input",
            outputVersion: "native-output",
            contentHash: String(repeating: "1", count: 64),
            location: nil,
            origin: "rust-receipt",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 1)
        ))
        try WorkflowSQLite.withDatabase(fileURL: repository.fileURL) { db in
            for kind in ["transcript", "chapters", "adSegments"] {
                try WorkflowSQLite.execute(
                    """
                    INSERT INTO artifacts(
                        kind,subject_id,input_version,output_version,content_hash,
                        location,origin,schema_version,integrity,verified_at,selected
                    ) VALUES(
                        '\(kind)','\(UUID().uuidString)','legacy-input','legacy-output',
                        'legacy-hash',NULL,'legacy',1,'available',1,1
                    )
                    """,
                    db
                )
            }
        }

        let selected = try repository.all()

        XCTAssertEqual(selected.map(\.kind), [.semanticIndex])
        XCTAssertEqual(selected.map(\.subjectID), [nativeSubjectID])
    }

    func testSharedCoreOwnedKindsAreNotRepresentableByGenericRepository() {
        for rawValue in ["transcript", "chapters", "adSegments"] {
            XCTAssertNil(ArtifactKind(rawValue: rawValue))
        }
    }

    func testSharedMigrationLockFencesGenericArtifactDatabaseWrites() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "artifact-migration-lock-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let persistence = Persistence(fileURL: directory.appendingPathComponent("state.json"))
        let repository = ArtifactRepository(fileURL: persistence.episodeStore.fileURL)
        let record = ArtifactRecord(
            kind: .metadataIndex,
            subjectID: UUID(),
            inputVersion: "input-v1",
            outputVersion: "output-v1",
            contentHash: String(repeating: "1", count: 64),
            location: nil,
            origin: "test",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 2)
        )
        let started = DispatchSemaphore(value: 0)
        let proceed = DispatchSemaphore(value: 0)
        let completed = DispatchSemaphore(value: 0)
        let succeeded = OSAllocatedUnfairLock(initialState: false)

        persistence.withSharedArtifactMigrationLock {
            DispatchQueue.global(qos: .userInitiated).async {
                started.signal()
                proceed.wait()
                let didWrite = (try? repository.adopt(record)) != nil
                succeeded.withLock { $0 = didWrite }
                completed.signal()
            }
            XCTAssertEqual(started.wait(timeout: .now() + 1), .success)
            proceed.signal()
            XCTAssertEqual(completed.wait(timeout: .now() + 0.1), .timedOut)
        }

        XCTAssertEqual(completed.wait(timeout: .now() + 2), .success)
        XCTAssertTrue(succeeded.withLock { $0 })
        XCTAssertEqual(try repository.current(kind: .metadataIndex, subjectID: record.subjectID), record)
    }
}
