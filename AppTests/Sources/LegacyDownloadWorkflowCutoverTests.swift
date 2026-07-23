import Foundation
import XCTest
@testable import Podcastr

final class LegacyDownloadWorkflowCutoverTests: XCTestCase {
    func testOversizedLegacyGenerationNormalizesWithoutChangingBackupEvidence() throws {
        let oversized = UInt64(Int64.max) + 2
        let backup = LegacyDownloadWorkflowBackup(
            formatVersion: 1,
            sourceGeneration: oversized,
            persistenceGeneration: 139,
            jobs: [],
            artifacts: [],
            tasks: [],
            candidates: []
        )
        let original = backup
        let normalized = backup.normalizedForStorage()

        XCTAssertEqual(
            normalized.sourceGeneration,
            LegacyDownloadWorkflowBackup.storageGeneration(oversized)
        )
        XCTAssertGreaterThan(normalized.sourceGeneration, 0)
        XCTAssertLessThanOrEqual(normalized.sourceGeneration, UInt64(Int64.max))
        XCTAssertEqual(backup, original)
        XCTAssertEqual(backup.sourceGeneration, oversized)
        XCTAssertEqual(normalized.jobs, backup.jobs)
        XCTAssertEqual(normalized.artifacts, backup.artifacts)
        XCTAssertEqual(normalized.tasks, backup.tasks)
        XCTAssertEqual(normalized.candidates, backup.candidates)
    }

    func testLegacySourceStoreAcceptsOnlyVerifiedStagedOutput() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let episodeID = UUID()
        let inputVersion = String(repeating: "a", count: 64)
        let bytes = Data("verified staged media".utf8)
        let attempt = root.appendingPathComponent("attempts", isDirectory: true)
            .appendingPathComponent(episodeID.uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: attempt, withIntermediateDirectories: true)
        let media = attempt.appendingPathComponent("transfer.media")
        try bytes.write(to: media)
        let manifest = StagedManifest(
            episodeID: episodeID,
            inputVersion: inputVersion,
            contentHash: ArtifactRepository.hash(bytes),
            byteCount: Int64(bytes.count),
            fileName: media.lastPathComponent,
            createdAt: Date(timeIntervalSince1970: 1_800_000_000)
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        try encoder.encode(manifest).write(to: attempt.appendingPathComponent("transfer.json"))
        let store = LegacyDownloadSourceStore(rootURL: root)

        let recovered = try XCTUnwrap(store.recoverableStagedOutput(
            episodeID: episodeID,
            inputVersion: inputVersion
        ))
        XCTAssertEqual(recovered.fileURL, media)
        XCTAssertEqual(recovered.byteCount, Int64(bytes.count))

        try Data("corrupt".utf8).write(to: media)
        XCTAssertNil(store.recoverableStagedOutput(
            episodeID: episodeID,
            inputVersion: inputVersion
        ))
    }

    func testResumeEvidenceRoundTripsWithoutCreatingPolicyState() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let episodeID = UUID()
        let resume = Data("opaque URLSession resume data".utf8)
        let store = LegacyDownloadSourceStore(rootURL: root)

        try store.writeResumeData(resume, episodeID: episodeID)

        XCTAssertEqual(store.loadResumeData(episodeID: episodeID), resume)
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: root.path),
            ["\(episodeID.uuidString).resume"]
        )
    }

    func testRetirementRequiresExactJobsAndArtifactsThenPreservesUnrelatedRows() throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let legacyRoot = try temporaryDirectory()
        defer {
            AppStateTestSupport.disposeIsolatedStore(at: fileURL)
            try? FileManager.default.removeItem(at: legacyRoot)
        }
        let episodeID = UUID()
        let episode = Episode(
            id: episodeID,
            podcastID: UUID(),
            guid: "legacy-download",
            title: "Legacy download",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.test/legacy.mp3")!
        )
        let jobStore = JobStore(fileURL: fileURL)
        _ = try jobStore.ensureJob(DesiredJob(
            idempotencyKey: "legacy-download-job",
            kind: .download,
            subjectID: episodeID,
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            resourceClass: .download
        ), notBefore: .distantPast)
        _ = try jobStore.ensureJob(DesiredJob(
            idempotencyKey: "unrelated-scheduled-job",
            kind: .scheduledAgentRun,
            subjectID: episodeID,
            inputVersion: "transcript-v1",
            resourceClass: .scheduledAgent
        ), notBefore: .distantPast)
        let repository = ArtifactRepository(fileURL: fileURL)
        try repository.adopt(ArtifactRecord(
            kind: .downloadFile,
            subjectID: episodeID,
            inputVersion: "legacy-input",
            outputVersion: "legacy-output",
            contentHash: String(repeating: "b", count: 64),
            location: "/legacy/audio.mp3",
            origin: "download",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 1_800_000_000)
        ))
        try repository.adopt(ArtifactRecord(
            kind: .metadataIndex,
            subjectID: episodeID,
            inputVersion: "metadata-input",
            outputVersion: "metadata-output",
            contentHash: String(repeating: "c", count: 64),
            location: nil,
            origin: "metadata",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date(timeIntervalSince1970: 1_800_000_001)
        ))
        var state = AppState()
        state.episodes = [episode]
        let snapshot = try LegacyDownloadWorkflowSnapshot.capture(
            state: state,
            jobStore: jobStore,
            artifactRepository: repository,
            tasks: [],
            producedResumeData: [:],
            downloadStore: LegacyDownloadSourceStore(rootURL: legacyRoot)
        )
        let mismatched = LegacyDownloadWorkflowBackup(
            formatVersion: snapshot.backup.formatVersion,
            sourceGeneration: snapshot.backup.sourceGeneration,
            persistenceGeneration: snapshot.backup.persistenceGeneration,
            jobs: snapshot.backup.jobs,
            artifacts: [],
            tasks: snapshot.backup.tasks,
            candidates: snapshot.backup.candidates
        )

        XCTAssertFalse(try jobStore.retireLegacyDownloadWorkflows(matching: mismatched))
        XCTAssertNotNil(try repository.current(kind: .downloadFile, subjectID: episodeID))
        XCTAssertTrue(try jobStore.retireLegacyDownloadWorkflows(matching: snapshot.backup))
        XCTAssertTrue(try jobStore.legacyDownloadWorkflowsAreRetired())
        XCTAssertNil(try repository.current(kind: .downloadFile, subjectID: episodeID))
        XCTAssertNotNil(try repository.current(kind: .metadataIndex, subjectID: episodeID))
        XCTAssertTrue(try jobStore.allJobs().contains {
            $0.idempotencyKey == "unrelated-scheduled-job"
        })
    }

    func testEpisodeEncodingCannotPersistTheRustOwnedDownloadProjection() throws {
        let episode = Episode(
            podcastID: UUID(),
            guid: "encoding-boundary",
            title: "Encoding boundary",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.test/audio.mp3")!,
            downloadState: .downloaded(
                localFileURL: URL(fileURLWithPath: "/tmp/legacy-download.mp3"),
                byteCount: 42
            )
        )

        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(episode)) as? [String: Any]
        )
        XCTAssertNil(object["downloadState"])
    }

    private func temporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-legacy-download-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}

private struct StagedManifest: Codable {
    let episodeID: UUID
    let inputVersion: String
    let contentHash: String
    let byteCount: Int64
    let fileName: String
    let createdAt: Date
}
