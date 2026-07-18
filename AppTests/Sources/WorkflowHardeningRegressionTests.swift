import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class WorkflowRepairRegressionTests: XCTestCase {
    private enum Damage { case missing, corrupt }

    private var stateURL: URL!
    private var artifactRoot: URL!
    private var appStore: AppStateStore!
    private var jobs: JobStore!
    private var artifacts: ArtifactRepository!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        stateURL = made.fileURL
        appStore = made.store
        let database = appStore.persistence.episodeStore.fileURL
        jobs = JobStore(fileURL: database)
        artifacts = ArtifactRepository(fileURL: database)
        try jobs.removeAll()
        artifactRoot = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: artifactRoot,
            withIntermediateDirectories: true
        )
    }

    override func tearDown() async throws {
        if let stateURL { AppStateTestSupport.disposeIsolatedStore(at: stateURL) }
        if let artifactRoot { try? FileManager.default.removeItem(at: artifactRoot) }
        artifacts = nil
        jobs = nil
        appStore = nil
        artifactRoot = nil
        stateURL = nil
        try await super.tearDown()
    }

    func testMissingSelectedArtifactRearmsAndConvergesThroughFencedLineage() throws {
        try assertRepairConverges(after: .missing)
    }

    func testCorruptSelectedArtifactRearmsAndConvergesThroughFencedLineage() throws {
        try assertRepairConverges(after: .corrupt)
    }

    private func assertRepairConverges(after damage: Damage) throws {
        let episode = makeEpisode(guid: "repair-\(UUID().uuidString)")
        appStore.upsertEpisodes([episode], forPodcast: episode.podcastID)

        let desired = try XCTUnwrap(DesiredStatePlanner().plan(.init(
            episodes: [episode],
            settings: appStore.state.settings,
            artifacts: [],
            transcriptDesiredEpisodeIDs: [episode.id],
            scheduledTasks: [],
            now: Date()
        )).first { $0.kind == .transcriptIngest })
        let transcript = Transcript(
            episodeID: episode.id,
            language: "en-US",
            source: .onDevice,
            segments: [.init(start: 0, end: 1, text: "Recovered")]
        )
        let data = try encode(transcript)
        let url = artifactRoot.appendingPathComponent("\(episode.id).json")
        try data.write(to: url)
        let record = ArtifactRecord(
            kind: .transcript,
            subjectID: episode.id,
            inputVersion: desired.inputVersion,
            outputVersion: ArtifactRepository.hash(data),
            contentHash: ArtifactRepository.hash(data),
            location: url.path,
            origin: TranscriptState.Source.onDevice.rawValue,
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date()
        )
        _ = try jobs.ensureJob(desired, notBefore: .distantPast)
        let firstAttempt = try XCTUnwrap(try jobs.claimDueJobs(
            resourceClass: desired.resourceClass,
            capacity: 1,
            now: Date(),
            owner: "original",
            leaseDuration: 60
        ).first)
        let originalToken = try XCTUnwrap(firstAttempt.leaseToken)
        try jobs.markRunning(id: firstAttempt.id, leaseToken: originalToken)
        try artifacts.commit(
            record,
            completingJobID: firstAttempt.id,
            leaseToken: originalToken
        )

        let reconciler = Reconciler(
            appStore: appStore,
            jobStore: jobs,
            artifacts: artifacts
        )
        _ = try reconciler.reconcile()
        XCTAssertEqual(appStore.episode(id: episode.id)?.transcriptState, .some(.ready(source: .onDevice)))
        let validTerminal = try XCTUnwrap(
            jobs.job(idempotencyKey: desired.idempotencyKey)
        )
        XCTAssertEqual(validTerminal.state, .succeeded)
        XCTAssertEqual(validTerminal.id, firstAttempt.id)
        XCTAssertEqual(validTerminal.attempt, 1)
        XCTAssertEqual(validTerminal.maxAttempts, desired.maxAttempts)

        switch damage {
        case .missing:
            try FileManager.default.removeItem(at: url)
        case .corrupt:
            try Data("corrupt".utf8).write(to: url)
        }

        XCTAssertEqual(try reconciler.reconcile().ensured, 1)
        XCTAssertEqual(appStore.episode(id: episode.id)?.transcriptState, .some(.none))
        let pendingRepair = try XCTUnwrap(
            jobs.job(idempotencyKey: desired.idempotencyKey)
        )
        XCTAssertEqual(pendingRepair.id, firstAttempt.id)
        XCTAssertEqual(pendingRepair.state, .pending)
        XCTAssertEqual(pendingRepair.attempt, 1)
        XCTAssertGreaterThan(pendingRepair.maxAttempts, validTerminal.maxAttempts)

        let repairAttempt = try XCTUnwrap(try jobs.claimDueJobs(
            resourceClass: desired.resourceClass,
            capacity: 1,
            now: Date(),
            owner: "repair",
            leaseDuration: 60
        ).first)
        let repairToken = try XCTUnwrap(repairAttempt.leaseToken)
        XCTAssertEqual(repairAttempt.id, firstAttempt.id)
        XCTAssertEqual(repairAttempt.attempt, 2)
        XCTAssertNotEqual(repairToken, originalToken)
        try jobs.markRunning(id: repairAttempt.id, leaseToken: repairToken)
        XCTAssertThrowsError(try artifacts.commit(
            record,
            completingJobID: repairAttempt.id,
            leaseToken: originalToken
        ))

        try data.write(to: url, options: .atomic)
        let repaired = ArtifactRecord(
            kind: record.kind,
            subjectID: record.subjectID,
            inputVersion: record.inputVersion,
            outputVersion: record.outputVersion,
            contentHash: record.contentHash,
            location: record.location,
            origin: record.origin,
            schemaVersion: record.schemaVersion,
            integrity: .available,
            verifiedAt: Date()
        )
        try artifacts.commit(
            repaired,
            completingJobID: repairAttempt.id,
            leaseToken: repairToken
        )

        XCTAssertEqual(try reconciler.reconcile().ensured, 0)
        let converged = try XCTUnwrap(
            jobs.job(idempotencyKey: desired.idempotencyKey)
        )
        XCTAssertEqual(converged.id, firstAttempt.id)
        XCTAssertEqual(converged.state, .succeeded)
        XCTAssertEqual(converged.attempt, 2)
        XCTAssertEqual(
            try jobs.allJobs().filter {
                $0.idempotencyKey == desired.idempotencyKey
            }.count,
            1
        )
        XCTAssertEqual(
            try artifacts.current(kind: .transcript, subjectID: episode.id)?.integrity,
            .available
        )
    }

    private func makeEpisode(guid: String) -> Episode {
        Episode(
            podcastID: UUID(),
            guid: guid,
            title: guid,
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }

    private func encode<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(value)
    }
}
