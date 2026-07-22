import Foundation
import XCTest
@testable import Podcastr

final class LegacyPublisherChapterWorkflowRetirementTests: XCTestCase {
    private var fileURL: URL!
    private var backupRoot: URL!
    private var store: JobStore!

    override func setUp() {
        super.setUp()
        fileURL = AppStateTestSupport.uniqueTempFileURL().appendingPathExtension("jobs")
        backupRoot = fileURL.appendingPathExtension("publisher-backups")
        store = JobStore(fileURL: fileURL)
    }

    override func tearDown() {
        AppStateTestSupport.disposeIsolatedStore(at: fileURL)
        try? FileManager.default.removeItem(at: backupRoot)
        store = nil
        fileURL = nil
        backupRoot = nil
        super.tearDown()
    }

    func testClassifiesEveryLegacyStateAndPreservesRawRows() throws {
        for (index, state) in WorkJobState.allCases.enumerated() {
            let input = "source-\(index)"
            try insert(LegacyChapterWorkflowTestSupport.makeJob(
                key: "publisher-\(state.rawValue)", kind: .publisherChapters,
                inputVersion: input, payload: try publisherPayload(inputVersion: input),
                state: state, attempt: state == .pending ? 0 : 1,
                leaseToken: state == .leased || state == .running ? UUID() : nil,
                leaseOwner: state == .leased || state == .running ? "legacy" : nil,
                leaseExpiresAt: state == .leased ? .distantPast : nil,
                externalProvider: "http", externalOperationID: "get-\(index)",
                externalOperationState: state.rawValue
            ))
        }
        try insert(LegacyChapterWorkflowTestSupport.makeJob(
            key: "publisher-corrupt", kind: .publisherChapters,
            inputVersion: "corrupt", payloadVersion: 99,
            payload: Data("not-json".utf8), state: .running, attempt: 1
        ))

        let jobs = try store.legacyChapterJobs(kind: .publisherChapters)
        let manifest = try LegacyPublisherChapterWorkflowBackupManifest(jobs: jobs)

        XCTAssertEqual(manifest.rows.map(\.job), jobs)
        for row in manifest.rows where row.job.idempotencyKey != "publisher-corrupt" {
            switch row.job.state {
            case .succeeded:
                XCTAssertEqual(row.classification, .completedEvidence)
            case .cancelled, .obsolete:
                XCTAssertEqual(row.classification, .cancelledOrObsoleteHistory)
            default:
                XCTAssertEqual(row.classification, .safeIdempotentRederivation)
            }
        }
        XCTAssertEqual(
            manifest.rows.first { $0.job.idempotencyKey == "publisher-corrupt" }?.classification,
            .corruptUnsupportedEvidence
        )
    }

    func testRetirementIsRestartSafeAndPreservesUnrelatedJobs() throws {
        let input = "publisher-source-v1"
        try insert(LegacyChapterWorkflowTestSupport.makeJob(
            key: "legacy-publisher", kind: .publisherChapters,
            inputVersion: input, payload: try publisherPayload(inputVersion: input),
            state: .retryScheduled, attempt: 2, notBefore: .distantFuture,
            externalProvider: "http", externalOperationID: "get-1"
        ))
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: "native-transcript", kind: .transcriptIngest,
            subjectID: UUID(), inputVersion: "audio-v1", resourceClass: .remoteSTT
        ))

        try LegacyPublisherChapterWorkflowRetirement.run(
            jobStore: store,
            backupRoot: backupRoot,
            modelSourceGeneration: 42,
            now: Date(timeIntervalSince1970: 1_000)
        )
        let marker = try XCTUnwrap(store.legacyChapterWorkflowRetirementMarker())
        XCTAssertEqual(marker.modelSourceGeneration, 42)
        XCTAssertTrue(try store.legacyChapterJobs(kind: .publisherChapters).isEmpty)
        XCTAssertNotNil(try store.job(idempotencyKey: "native-transcript"))
        let backup = try XCTUnwrap(LegacyPublisherChapterWorkflowBackupManifest.load(
            from: backupRoot,
            sourceGeneration: marker.publisherSourceGeneration
        ))
        XCTAssertEqual(backup.rows.map(\.job.idempotencyKey), ["legacy-publisher"])

        try LegacyPublisherChapterWorkflowRetirement.run(
            jobStore: store,
            backupRoot: backupRoot,
            modelSourceGeneration: 42,
            now: Date(timeIntervalSince1970: 2_000)
        )
        XCTAssertEqual(try store.legacyChapterWorkflowRetirementMarker(), marker)
    }

    func testExactCommitRejectsLateRowsWithoutDeletingOrMarking() throws {
        let input = "source-v1"
        try insert(LegacyChapterWorkflowTestSupport.makeJob(
            key: "verified", kind: .publisherChapters, inputVersion: input,
            payload: try publisherPayload(inputVersion: input)
        ))
        let verified = try store.legacyChapterJobs(kind: .publisherChapters)
        let manifest = try LegacyPublisherChapterWorkflowBackupManifest(jobs: verified)
        try insert(LegacyChapterWorkflowTestSupport.makeJob(
            key: "late", kind: .publisherChapters, inputVersion: input,
            payload: try publisherPayload(inputVersion: input)
        ))
        let marker = LegacyChapterWorkflowRetirementMarker(
            modelSourceGeneration: 9,
            publisherSourceGeneration: manifest.sourceGeneration,
            publisherSourceFingerprint: manifest.sourceFingerprint,
            completedAt: Date(timeIntervalSince1970: 1_000)
        )

        XCTAssertFalse(try store.commitLegacyChapterWorkflowRetirement(
            expectedPublisherJobs: verified,
            marker: marker
        ))
        XCTAssertEqual(try store.legacyChapterJobs(kind: .publisherChapters).count, 2)
        XCTAssertNil(try store.legacyChapterWorkflowRetirementMarker())
    }

    func testModelRowsFenceFinalRetirementMarker() throws {
        try insert(LegacyChapterWorkflowTestSupport.makeJob(
            key: "legacy-model",
            kind: .chapterArtifacts
        ))
        let empty = try LegacyPublisherChapterWorkflowBackupManifest(jobs: [])
        let marker = LegacyChapterWorkflowRetirementMarker(
            modelSourceGeneration: 9,
            publisherSourceGeneration: empty.sourceGeneration,
            publisherSourceFingerprint: empty.sourceFingerprint,
            completedAt: Date(timeIntervalSince1970: 1_000)
        )

        XCTAssertFalse(try store.commitLegacyChapterWorkflowRetirement(
            expectedPublisherJobs: [],
            marker: marker
        ))
        XCTAssertNotNil(try store.legacyChapterJobs(kind: .chapterArtifacts).first)
        XCTAssertNil(try store.legacyChapterWorkflowRetirementMarker())
    }

    func testGenericRecoveryAndClaimingNeverMutateLegacyKinds() throws {
        let publisher = LegacyChapterWorkflowTestSupport.makeJob(
            key: "legacy-publisher", kind: .publisherChapters,
            inputVersion: "source", payload: try publisherPayload(inputVersion: "source"),
            state: .blocked, attempt: 1, lastErrorClass: .missingDependency
        )
        let model = LegacyChapterWorkflowTestSupport.makeJob(
            key: "legacy-model", kind: .chapterArtifacts,
            state: .running, attempt: 1, leaseToken: UUID(), leaseOwner: "old",
            leaseExpiresAt: .distantPast
        )
        try insert(publisher)
        try insert(model)
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: "native-feed", kind: .feedDiscovery,
            subjectID: UUID(), inputVersion: "feed-v1", resourceClass: .planning
        ), notBefore: .distantPast)

        try store.unblockAll()
        try store.reclaimExpiredLeases()
        let claimed = try store.claimDueJobs(
            resourceClass: .planning, capacity: 3, now: Date(),
            owner: "new", leaseDuration: 60
        )

        XCTAssertEqual(claimed.map(\.idempotencyKey), ["native-feed"])
        XCTAssertEqual(try store.legacyChapterJobs(kind: .publisherChapters), [publisher])
        XCTAssertEqual(try store.legacyChapterJobs(kind: .chapterArtifacts), [model])
    }

    func testRestartFailsClosedWhenPublishedBackupIsCorrupt() throws {
        let input = "source-v1"
        try insert(LegacyChapterWorkflowTestSupport.makeJob(
            key: "legacy-publisher", kind: .publisherChapters,
            inputVersion: input, payload: try publisherPayload(inputVersion: input)
        ))
        try LegacyPublisherChapterWorkflowRetirement.run(
            jobStore: store, backupRoot: backupRoot, modelSourceGeneration: 7
        )
        let backupURL = try XCTUnwrap(FileManager.default.contentsOfDirectory(
            at: backupRoot,
            includingPropertiesForKeys: nil
        ).first { $0.pathExtension == "json" })
        try Data("corrupt".utf8).write(to: backupURL, options: .atomic)

        XCTAssertThrowsError(try LegacyPublisherChapterWorkflowRetirement.run(
            jobStore: store, backupRoot: backupRoot, modelSourceGeneration: 7
        )) { error in
            XCTAssertEqual(
                error as? LegacyChapterWorkflowBackupError,
                .invalidBackup
            )
        }
    }

    private func insert(_ job: LegacyChapterWorkflowJob) throws {
        try LegacyChapterWorkflowTestSupport.insert(job, into: store)
    }

    private func publisherPayload(inputVersion: String) throws -> Data {
        try JSONSerialization.data(withJSONObject: [
            "url": "https://example.com/chapters.json",
            "sourceVersion": inputVersion,
        ], options: [.sortedKeys])
    }
}
