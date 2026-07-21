import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class LegacyModelChapterWorkflowSnapshotTests: XCTestCase {
    func testPreservesTerminalEvidenceAndQuarantinesUncertainAttempts() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL().appendingPathExtension("jobs")
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        let store = JobStore(fileURL: fileURL)
        let pendingID = UUID()
        try insert(
            store,
            episodeID: pendingID,
            key: "pending",
            inputVersion: "pending-v1",
            notBefore: .distantFuture
        )

        let blockedID = UUID()
        let blocked = try claim(store, episodeID: blockedID, key: "blocked", inputVersion: "blocked-v1")
        try store.markBlocked(
            id: blocked.id,
            leaseToken: try XCTUnwrap(blocked.leaseToken),
            reason: JobFailure(classification: .missingCredential, message: "credential missing")
        )

        let failedID = UUID()
        let failed = try claim(store, episodeID: failedID, key: "failed", inputVersion: "failed-v1")
        try store.markFailedPermanent(
            id: failed.id,
            leaseToken: try XCTUnwrap(failed.leaseToken),
            error: JobFailure(classification: .network, message: "provider unreachable")
        )

        let cancelledID = UUID()
        let cancelled = try claim(
            store,
            episodeID: cancelledID,
            key: "cancelled",
            inputVersion: "cancelled-v1"
        )
        try store.markCancelled(
            id: cancelled.id,
            leaseToken: try XCTUnwrap(cancelled.leaseToken)
        )

        let successID = UUID()
        let artifactID = UUID()
        let contentDigest = String(repeating: "a", count: 64)
        let integrityDigest = String(repeating: "b", count: 64)
        let succeeded = try claim(
            store,
            episodeID: successID,
            key: "succeeded",
            inputVersion: "succeeded-v1"
        )
        let receipt = SharedChapterWorkflowReceipt(
            schemaVersion: SharedChapterWorkflowReceipt.currentSchemaVersion,
            episodeID: successID,
            inputVersion: "succeeded-v1",
            artifactID: artifactID.uuidString,
            contentDigest: contentDigest,
            integrityDigest: integrityDigest,
            selectionRevision: 7
        )
        try store.complete(
            id: succeeded.id,
            leaseToken: try XCTUnwrap(succeeded.leaseToken),
            outputVersion: try JSONEncoder().encode(receipt).base64EncodedString()
        )

        let runningID = UUID()
        let running = try claim(store, episodeID: runningID, key: "running", inputVersion: "running-v1")
        try store.markRunning(id: running.id, leaseToken: try XCTUnwrap(running.leaseToken))

        let snapshot = try LegacyModelChapterWorkflowSnapshot.capture(from: store)
        XCTAssertEqual(
            snapshot,
            try LegacyModelChapterWorkflowSnapshot.capture(from: store),
            "An unchanged legacy store must produce the same restart generation"
        )
        XCTAssertNil(candidate(pendingID, in: snapshot))
        XCTAssertEqual(candidate(runningID, in: snapshot)?.disposition, .ambiguous)
        XCTAssertEqual(
            candidate(blockedID, in: snapshot)?.disposition,
            .blocked(
                failureCode: "missing_credential",
                failureDetail: "credential missing",
                mayHaveSubmitted: false
            )
        )
        XCTAssertEqual(
            candidate(failedID, in: snapshot)?.disposition,
            .failed(
                failureCode: "transport",
                failureDetail: "provider unreachable",
                mayHaveSubmitted: true
            )
        )
        XCTAssertEqual(
            candidate(cancelledID, in: snapshot)?.disposition,
            .cancelled(mayHaveSubmitted: true)
        )
        XCTAssertEqual(
            candidate(successID, in: snapshot)?.disposition,
            .succeeded(
                artifactId: ChapterArtifactId(uuid: artifactID),
                contentDigest: try XCTUnwrap(ContentDigest(hexadecimal: contentDigest)),
                integrityDigest: try XCTUnwrap(ContentDigest(hexadecimal: integrityDigest)),
                selectionRevision: StateRevision(value: 7)
            )
        )
        XCTAssertEqual(snapshot.backup.rows.count, 6)
        XCTAssertEqual(
            snapshot.backup.rows.first { $0.job.subjectID == pendingID }?.classification,
            .pendingUnattempted
        )
        XCTAssertEqual(
            snapshot.backup.rows.first { $0.job.subjectID == runningID }?.classification,
            .ambiguousSubmission
        )
    }

    func testBackupIsNoClobberIntegrityCheckedAndContentQualified() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL().appendingPathExtension("jobs")
        let backupRoot = fileURL.appendingPathExtension("model-backups")
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        defer { try? FileManager.default.removeItem(at: backupRoot) }
        let store = JobStore(fileURL: fileURL)
        try insert(
            store,
            episodeID: UUID(),
            key: "pending-backup",
            inputVersion: "backup-v1",
            notBefore: .distantFuture
        )
        let snapshot = try LegacyModelChapterWorkflowSnapshot.capture(from: store)

        try snapshot.backup.publish(to: backupRoot)
        try snapshot.backup.publish(to: backupRoot)
        XCTAssertEqual(
            try LegacyModelChapterWorkflowBackupManifest.load(
                from: backupRoot,
                sourceGeneration: snapshot.sourceGeneration
            ),
            snapshot.backup
        )
        let leftover = backupRoot.appendingPathComponent(
            ".model-chapter-workflows-interrupted.tmp"
        )
        try Data("partial".utf8).write(to: leftover)
        XCTAssertEqual(
            try LegacyModelChapterWorkflowBackupManifest.load(
                from: backupRoot,
                sourceGeneration: snapshot.sourceGeneration
            ),
            snapshot.backup
        )
        let backupURL = try XCTUnwrap(
            FileManager.default.contentsOfDirectory(
                at: backupRoot,
                includingPropertiesForKeys: nil
            ).first { $0.pathExtension == "json" }
        )
        try Data("tampered".utf8).write(to: backupURL, options: .atomic)
        XCTAssertThrowsError(try LegacyModelChapterWorkflowBackupManifest.load(
            from: backupRoot,
            sourceGeneration: snapshot.sourceGeneration
        ))
        XCTAssertThrowsError(try snapshot.backup.publish(to: backupRoot))
    }

    func testSourceFingerprintIncludesFieldsOutsideCutoverDisposition() throws {
        let baseline = try fingerprint(makeFingerprintJob())
        let variants = [
            makeFingerprintJob(kind: .publisherChapters),
            makeFingerprintJob(occurrenceID: "occurrence-2"),
            makeFingerprintJob(payloadVersion: 2),
            makeFingerprintJob(payload: Data("payload-2".utf8)),
            makeFingerprintJob(priority: 11),
            makeFingerprintJob(resourceClass: .embedding),
        ]

        XCTAssertEqual(Set(try variants.map(fingerprint)).count, variants.count)
        XCTAssertTrue(try variants.allSatisfy { try fingerprint($0) != baseline })
    }

    func testCompareAndDeleteRejectsRowsAddedAfterVerification() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL().appendingPathExtension("jobs")
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        let store = JobStore(fileURL: fileURL)
        try insert(
            store,
            episodeID: UUID(),
            key: "verified-row",
            inputVersion: "verified-v1"
        )
        let verified = try store.allJobs().filter { $0.kind == .chapterArtifacts }
        try insert(
            store,
            episodeID: UUID(),
            key: "late-row",
            inputVersion: "late-v1"
        )

        XCTAssertFalse(try store.removeJobs(kind: .chapterArtifacts, matching: verified))
        let current = try store.allJobs().filter { $0.kind == .chapterArtifacts }
        XCTAssertEqual(current.count, 2)
        XCTAssertTrue(try store.removeJobs(kind: .chapterArtifacts, matching: current))
        XCTAssertTrue(try store.allJobs().allSatisfy { $0.kind != .chapterArtifacts })
    }

    private func insert(
        _ store: JobStore,
        episodeID: UUID,
        key: String,
        inputVersion: String,
        notBefore: Date = .distantPast
    ) throws {
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: key,
            kind: .chapterArtifacts,
            subjectID: episodeID,
            inputVersion: inputVersion,
            resourceClass: .utilityLLM
        ), notBefore: notBefore)
    }

    private func claim(
        _ store: JobStore,
        episodeID: UUID,
        key: String,
        inputVersion: String
    ) throws -> WorkJob {
        try insert(store, episodeID: episodeID, key: key, inputVersion: inputVersion)
        return try XCTUnwrap(try store.claimDueJobs(
            resourceClass: .utilityLLM,
            capacity: 1,
            now: Date(),
            owner: "snapshot-test",
            leaseDuration: 60
        ).first)
    }

    private func candidate(
        _ episodeID: UUID,
        in snapshot: LegacyModelChapterWorkflowSnapshot
    ) -> LegacyModelChapterCutoverCandidate? {
        snapshot.candidates.first { $0.episodeId == EpisodeId(uuid: episodeID) }
    }

    private func fingerprint(_ job: WorkJob) throws -> String {
        try LegacyModelChapterWorkflowSnapshot.sourceFingerprint(for: [job])
    }

    private func makeFingerprintJob(
        kind: WorkJobKind = .chapterArtifacts,
        occurrenceID: String? = "occurrence-1",
        payloadVersion: Int = 1,
        payload: Data? = Data("payload-1".utf8),
        priority: Int = 10,
        resourceClass: WorkResourceClass = .utilityLLM
    ) -> WorkJob {
        WorkJob(
            id: UUID(uuidString: "11111111-1111-1111-1111-111111111111")!,
            idempotencyKey: "fingerprint-job",
            kind: kind,
            subjectID: UUID(uuidString: "22222222-2222-2222-2222-222222222222")!,
            inputVersion: "input-v1",
            occurrenceID: occurrenceID,
            payloadVersion: payloadVersion,
            payload: payload,
            state: .blocked,
            priority: priority,
            resourceClass: resourceClass,
            attempt: 1,
            maxAttempts: 8,
            notBefore: Date(timeIntervalSince1970: 100),
            leaseToken: nil,
            leaseOwner: nil,
            leaseExpiresAt: nil,
            externalProvider: "openrouter",
            externalOperationID: "operation-1",
            externalOperationState: "accepted",
            outputVersion: nil,
            lastErrorClass: .rateLimited,
            lastErrorMessage: "retry later",
            createdAt: Date(timeIntervalSince1970: 90),
            updatedAt: Date(timeIntervalSince1970: 100)
        )
    }
}
