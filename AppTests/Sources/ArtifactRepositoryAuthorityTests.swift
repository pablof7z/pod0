import Foundation
import XCTest
import os
@testable import Podcastr

final class ArtifactRepositoryAuthorityTests: XCTestCase {
    func testGenericSwiftRepositoryRejectsEveryTranscriptMutation() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "artifact-authority-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let repository = ArtifactRepository(fileURL: directory.appendingPathComponent("jobs.sqlite"))
        let record = ArtifactRecord(
            kind: .transcript,
            subjectID: UUID(),
            inputVersion: "legacy-input",
            outputVersion: "legacy-output",
            contentHash: String(repeating: "0", count: 64),
            location: nil,
            origin: "publisher",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 1)
        )

        XCTAssertThrowsError(try repository.adopt(record)) { error in
            XCTAssertEqual(
                error as? ArtifactRepositoryAuthorityError,
                .sharedCoreOwnsTranscripts
            )
        }
        XCTAssertThrowsError(
            try repository.commit(
                record,
                completingJobID: UUID(),
                leaseToken: UUID()
            )
        )
        XCTAssertThrowsError(
            try repository.markIntegrity(
                kind: .transcript,
                subjectID: record.subjectID,
                integrity: .corrupt
            )
        )
        XCTAssertFalse(FileManager.default.fileExists(atPath: repository.fileURL.path))
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
