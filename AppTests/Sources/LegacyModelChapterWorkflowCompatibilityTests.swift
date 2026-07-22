import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class LegacyModelChapterWorkflowCompatibilityTests: XCTestCase {
    func testEveryLegacyLifecycleStateAndLeaseAgeHasDeterministicDisposition() {
        let now = Date(timeIntervalSince1970: 10_000)
        let cases: [(LegacyChapterWorkflowJob, LegacyModelChapterCutoverDisposition?)] = [
            (job(key: "pending-new", state: .pending), nil),
            (job(key: "pending-attempted", state: .pending, attempt: 1), .ambiguous),
            (job(
                key: "leased-live", state: .leased, attempt: 1,
                leaseToken: UUID(), leaseExpiresAt: now.addingTimeInterval(60)
            ), .ambiguous),
            (job(
                key: "leased-expired", state: .leased, attempt: 1,
                leaseToken: UUID(), leaseExpiresAt: now.addingTimeInterval(-60)
            ), .ambiguous),
            (job(key: "running", state: .running, attempt: 1), .ambiguous),
            (job(key: "retry", state: .retryScheduled, attempt: 1), .ambiguous),
            (job(
                key: "blocked-safe", state: .blocked, attempt: 1,
                error: .missingDependency
            ), .blocked(
                failureCode: "storage_unavailable",
                failureDetail: "legacy failure",
                mayHaveSubmitted: false
            )),
            (job(
                key: "blocked-unsafe", state: .blocked, attempt: 1,
                error: .unsafeToRetry
            ), .ambiguous),
            (job(
                key: "failed", state: .failedPermanent, attempt: 1,
                error: .network
            ), .failed(
                failureCode: "transport",
                failureDetail: "legacy failure",
                mayHaveSubmitted: true
            )),
            (job(key: "cancelled", state: .cancelled, attempt: 1), .cancelled(
                mayHaveSubmitted: true
            )),
            (job(key: "obsolete-new", state: .obsolete), nil),
            (job(key: "obsolete-attempted", state: .obsolete, attempt: 1), .ambiguous),
            (job(key: "succeeded-without-receipt", state: .succeeded, attempt: 1), .ambiguous),
        ]

        for (job, expected) in cases {
            XCTAssertEqual(
                LegacyModelChapterWorkflowSnapshot.candidate(job)?.disposition,
                expected,
                job.idempotencyKey
            )
        }
    }

    func testOldUnknownAndCorruptReceiptVersionsRemainAmbiguous() throws {
        for schemaVersion in [0, 2, 99] {
            let receipt = LegacySharedChapterWorkflowReceiptV1(
                schemaVersion: schemaVersion,
                episodeID: episodeID,
                inputVersion: "source-v1",
                artifactID: UUID().uuidString,
                contentDigest: String(repeating: "a", count: 64),
                integrityDigest: String(repeating: "b", count: 64),
                selectionRevision: 1
            )
            let candidate = LegacyModelChapterWorkflowSnapshot.candidate(job(
                key: "receipt-\(schemaVersion)",
                state: .succeeded,
                attempt: 1,
                outputVersion: try JSONEncoder().encode(receipt).base64EncodedString()
            ))
            XCTAssertEqual(candidate?.disposition, .ambiguous)
        }
        XCTAssertEqual(
            LegacyModelChapterWorkflowSnapshot.candidate(job(
                key: "corrupt-receipt", state: .succeeded, attempt: 1,
                outputVersion: "not-base64"
            ))?.disposition,
            .ambiguous
        )
    }

    func testDuplicateEpisodeRowsRemainOrderedFullEvidence() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL().appendingPathExtension("jobs")
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        let store = JobStore(fileURL: fileURL)
        let laterID = UUID(uuidString: "ffffffff-ffff-ffff-ffff-ffffffffffff")!
        let earlierID = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!
        try LegacyChapterWorkflowTestSupport.insert(job(
            id: laterID, key: "duplicate-later", state: .running, attempt: 1
        ), into: store)
        try LegacyChapterWorkflowTestSupport.insert(job(
            id: earlierID, key: "duplicate-earlier", state: .retryScheduled, attempt: 2
        ), into: store)

        let snapshot = try LegacyModelChapterWorkflowSnapshot.capture(from: store)

        XCTAssertEqual(snapshot.backup.rows.map(\.job.id), [earlierID, laterID])
        XCTAssertEqual(snapshot.candidates.map(\.episodeId), [
            EpisodeId(uuid: episodeID),
            EpisodeId(uuid: episodeID),
        ])
        XCTAssertEqual(snapshot, try LegacyModelChapterWorkflowSnapshot.capture(from: store))
    }

    func testV1ManifestKeepsThePreCutoverJSONShape() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL().appendingPathExtension("jobs")
        let backupRoot = fileURL.appendingPathExtension("model-backups")
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        defer { try? FileManager.default.removeItem(at: backupRoot) }
        let store = JobStore(fileURL: fileURL)
        try LegacyChapterWorkflowTestSupport.insert(job(
            key: "v1-shape", state: .running, attempt: 1
        ), into: store)
        let snapshot = try LegacyModelChapterWorkflowSnapshot.capture(from: store)
        try snapshot.backup.publish(to: backupRoot)
        let file = try XCTUnwrap(FileManager.default.contentsOfDirectory(
            at: backupRoot,
            includingPropertiesForKeys: nil
        ).first { $0.pathExtension == "json" })
        let envelope = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any]
        )
        let manifest = try XCTUnwrap(envelope["manifest"] as? [String: Any])
        let rows = try XCTUnwrap(manifest["rows"] as? [[String: Any]])
        let encodedJob = try XCTUnwrap(rows.first?["job"] as? [String: Any])

        XCTAssertEqual(manifest["schemaVersion"] as? Int, 1)
        XCTAssertEqual(encodedJob["kind"] as? String, "chapterArtifacts")
        XCTAssertEqual(encodedJob["idempotencyKey"] as? String, "v1-shape")
        XCTAssertEqual(try LegacyModelChapterWorkflowBackupManifest.load(
            from: backupRoot,
            sourceGeneration: snapshot.sourceGeneration
        ), snapshot.backup)
    }

    private func job(
        id: UUID = UUID(),
        key: String,
        state: WorkJobState,
        attempt: Int = 0,
        leaseToken: UUID? = nil,
        leaseExpiresAt: Date? = nil,
        error: JobErrorClass? = nil,
        outputVersion: String? = nil
    ) -> LegacyChapterWorkflowJob {
        LegacyChapterWorkflowTestSupport.makeJob(
            id: id,
            key: key,
            episodeID: episodeID,
            inputVersion: "source-v1",
            state: state,
            attempt: attempt,
            leaseToken: leaseToken,
            leaseOwner: leaseToken == nil ? nil : "legacy-owner",
            leaseExpiresAt: leaseExpiresAt,
            outputVersion: outputVersion,
            lastErrorClass: error,
            lastErrorMessage: error == nil ? nil : "legacy failure"
        )
    }

    private let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
}
