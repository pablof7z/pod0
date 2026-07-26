import Foundation
import XCTest
@testable import Podcastr

final class LegacyFeedDiscoveryWorkflowRetirementTests: XCTestCase {
    func testBackupEvidenceVerifiesAndRejectsConflictingOrCorruptBytes() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-feed-cutover-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let backup = LegacyFeedDiscoveryWorkflowBackup(
            formatVersion: 1,
            persistenceGeneration: 4,
            capturedAt: Date(timeIntervalSince1970: 100),
            notificationsEnabled: false,
            jobs: [],
            artifacts: []
        )
        let evidence = try backup.evidence()
        let destination = try backup.publish(to: root, sourceGeneration: 9)

        XCTAssertEqual(try LegacyFeedDiscoveryWorkflowBackup.load(
            from: root,
            sourceGeneration: 9,
            expectedDigest: evidence.digest,
            expectedByteCount: evidence.byteCount
        ), backup)

        let conflicting = LegacyFeedDiscoveryWorkflowBackup(
            formatVersion: 1,
            persistenceGeneration: 4,
            capturedAt: Date(timeIntervalSince1970: 101),
            notificationsEnabled: false,
            jobs: [],
            artifacts: []
        )
        XCTAssertThrowsError(try conflicting.publish(to: root, sourceGeneration: 9)) {
            XCTAssertEqual(
                $0 as? LegacyFeedDiscoveryWorkflowBackupError,
                .backupConflict
            )
        }

        try Data("corrupt".utf8).write(to: destination, options: .atomic)
        XCTAssertThrowsError(try LegacyFeedDiscoveryWorkflowBackup.load(
            from: root,
            sourceGeneration: 9,
            expectedDigest: evidence.digest,
            expectedByteCount: evidence.byteCount
        ))
    }

    func testExactRetirementPreservesUnrelatedRowsAndIsIdempotent() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        let store = JobStore(fileURL: fileURL)
        let repository = ArtifactRepository(fileURL: fileURL)
        let episodeID = UUID()
        let legacy = LegacyFeedDiscoveryWorkflowTestSupport.makeJob(
            kind: .newEpisodeNotification,
            subjectID: episodeID,
            inputVersion: String(repeating: "a", count: 64),
            occurrenceID: "notification:legacy",
            payload: Data("{}".utf8)
        )
        try LegacyFeedDiscoveryWorkflowTestSupport.insert(legacy, into: store)
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: "unrelated-metadata",
            kind: .metadataIndex,
            subjectID: episodeID,
            inputVersion: "metadata-v1",
            resourceClass: .embedding
        ))
        try LegacyFeedDiscoveryWorkflowTestSupport.insert(
            LegacyFeedDiscoveryArtifactRecord(
            kind: .notificationDelivery,
            subjectID: episodeID,
            inputVersion: legacy.inputVersion,
            outputVersion: "delivered",
            contentHash: String(repeating: "b", count: 64),
            location: nil,
            origin: "notification",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 200)
            ),
            into: store
        )
        try repository.adopt(ArtifactRecord(
            kind: .metadataIndex,
            subjectID: episodeID,
            inputVersion: "metadata-v1",
            outputVersion: "indexed",
            contentHash: String(repeating: "c", count: 64),
            location: nil,
            origin: "metadata",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 201)
        ))
        let backup = LegacyFeedDiscoveryWorkflowBackup(
            formatVersion: 1,
            persistenceGeneration: 9,
            capturedAt: Date(timeIntervalSince1970: 300),
            notificationsEnabled: true,
            jobs: try store.legacyFeedDiscoveryJobs(),
            artifacts: try store.legacyFeedDiscoveryArtifacts()
        )
        let digest = try backup.evidence().digest.stableString

        XCTAssertFalse(try store.retireLegacyFeedDiscovery(
            matching: LegacyFeedDiscoveryWorkflowBackup(
                formatVersion: 1,
                persistenceGeneration: 9,
                capturedAt: backup.capturedAt,
                notificationsEnabled: true,
                jobs: [],
                artifacts: backup.artifacts
            ),
            sourceGeneration: 10,
            sourceDigest: digest
        ))
        XCTAssertTrue(try store.retireLegacyFeedDiscovery(
            matching: backup,
            sourceGeneration: 10,
            sourceDigest: digest
        ))
        XCTAssertTrue(try store.legacyFeedDiscoverySourceIsRetired())
        XCTAssertTrue(try store.retireLegacyFeedDiscovery(
            matching: backup,
            sourceGeneration: 10,
            sourceDigest: digest
        ))
        XCTAssertTrue(try store.allJobs().contains {
            $0.idempotencyKey == "unrelated-metadata"
        })
        XCTAssertNotNil(try repository.current(kind: .metadataIndex, subjectID: episodeID))
        XCTAssertTrue(try store.legacyFeedDiscoveryArtifacts().isEmpty)
    }

    func testRetiredKindsAreUnrepresentableByNativeJobStore() {
        XCTAssertNil(WorkJobKind(rawValue: "feedDiscovery"))
        XCTAssertNil(WorkJobKind(rawValue: "newEpisodeNotification"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("feedDiscovery"))
        XCTAssertFalse(JobStore.supportedKindSQL.contains("newEpisodeNotification"))
    }
}
