import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class LegacyTranscriptWorkflowCutoverTests: XCTestCase {
    func testBootstrapCutsPendingTranscriptJobOverAndRetiresExactSwiftRow() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        let store = JobStore(fileURL: fixture.persistence.episodeStore.fileURL)
        let episode = try XCTUnwrap(
            try fixture.persistence.load().episodes.first { $0.id == fixture.episodeID }
        )
        let payload = TranscriptJobPayload(
            provider: .elevenLabsScribe,
            modelID: "scribe_v1",
            audioURL: episode.enclosureURL,
            audioVersion: DesiredStatePlanner.audioVersion(episode),
            userInitiated: true
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: "legacy-transcript-pending",
            kind: .transcriptIngest,
            subjectID: episode.id,
            inputVersion: payload.audioVersion,
            payload: try encoder.encode(payload),
            resourceClass: .remoteSTT
        ))

        let client = try bootstrap(fixture)
        defer { client.shutdown() }

        let cutover = client.facade.transcriptWorkflowCutover()
        XCTAssertEqual(cutover.stage, .authoritative)
        XCTAssertEqual(cutover.rowCount, 1)
        XCTAssertEqual(cutover.adoptedWorkflowCount, 1)
        XCTAssertTrue(try store.legacyTranscriptJobsAreRetired())
        let generation = try XCTUnwrap(cutover.sourceGeneration)
        let backup = try LegacyTranscriptWorkflowBackupManifest.load(
            from: fixture.persistence.legacyTranscriptWorkflowBackupRootURL,
            sourceGeneration: generation
        )
        XCTAssertEqual(backup.rows.count, 1)
        XCTAssertEqual(backup.rows.first?.classification, .restart)
        XCTAssertEqual(backup.rows.first?.job.idempotencyKey, "legacy-transcript-pending")
        XCTAssertFalse(JobStore.supportedKindSQL.contains("transcriptIngest"))

        let envelope = client.facade.snapshot(request: ProjectionRequest(
            scope: .transcriptWorkflows(episodeId: EpisodeId(uuid: episode.id)),
            offset: 0,
            maxItems: 1
        ))
        guard case .transcriptWorkflows(let projection) = envelope.projection else {
            return XCTFail("Expected Rust transcript workflow projection")
        }
        XCTAssertEqual(projection.workflows.first?.episodeId.uuid, episode.id)
    }

    func testBackupIsImmutableAndExactDeleteRejectsSourceChange() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL().appendingPathExtension("jobs")
        let backupRoot = fileURL.appendingPathExtension("transcript-workflow-backups")
        defer { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        defer { try? FileManager.default.removeItem(at: backupRoot) }
        let store = JobStore(fileURL: fileURL)
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: "legacy-one",
            kind: .transcriptIngest,
            subjectID: UUID(),
            inputVersion: "source-v1",
            resourceClass: .remoteSTT
        ))
        let original = try store.legacyTranscriptJobs()
        let rows = original.map {
            LegacyTranscriptWorkflowBackupRow(job: $0, classification: .obsolete)
        }
        let coreRows = try rows.map { try $0.coreValue() }
        let fingerprint = LegacyTranscriptWorkflowBackupManifest.sourceFingerprint(for: coreRows)
        let manifest = LegacyTranscriptWorkflowBackupManifest(
            sourceGeneration: LegacyTranscriptWorkflowBackupManifest.sourceGeneration(
                for: fingerprint
            ),
            sourceFingerprint: fingerprint.stableString,
            rows: rows
        )
        let evidence = try manifest.publish(to: backupRoot)
        XCTAssertGreaterThan(evidence.byteCount, UInt64(coreRows[0].rowBytes.count))
        XCTAssertEqual(
            try LegacyTranscriptWorkflowBackupManifest.load(
                from: backupRoot,
                sourceGeneration: manifest.sourceGeneration
            ),
            manifest
        )

        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: "legacy-late",
            kind: .transcriptIndex,
            subjectID: UUID(),
            inputVersion: "index-v1",
            resourceClass: .embedding
        ))
        XCTAssertFalse(try store.removeLegacyTranscriptJobs(matching: original))
        XCTAssertEqual(try store.legacyTranscriptJobs().count, 2)
    }

    private func bootstrap(
        _ fixture: SharedTranscriptRecoveryTestSupport.Fixture
    ) throws -> SharedLibraryClient {
        switch SharedLibraryBootstrap.run(
            persistence: fixture.persistence,
            legacyState: try fixture.persistence.load(),
            feedHost: QueuedCoreFeedHost([])
        ) {
        case .ready(let client): client
        case .authoritativeUnavailable(let reason, let stage):
            throw Failure.bootstrap("\(stage.rawValue):\(reason)")
        }
    }

    private enum Failure: Error { case bootstrap(String) }
}
