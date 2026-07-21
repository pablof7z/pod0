import CSQLite3
import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class LegacyModelChapterWorkflowCutoverTests: XCTestCase {
    private let configuredModel = "ollama:llama3.2"

    func testBootstrapResumesStagedCutoverDeletesLegacyRowsAndNeverReposts() throws {
        let prepared = try prepareInterruptedCutover()
        defer { SharedTranscriptRecoveryTestSupport.dispose(prepared.fixture) }
        XCTAssertEqual(try prepared.jobs.allJobs().filter { $0.kind == .chapterArtifacts }.count, 1)

        let client = try bootstrap(prepared.fixture)
        defer { client.shutdown() }

        assertAuthoritativeAmbiguity(client, episodeID: prepared.fixture.episodeID)
        XCTAssertTrue(try prepared.jobs.allJobs().allSatisfy { $0.kind != .chapterArtifacts })
        XCTAssertEqual(
            try LegacyModelChapterWorkflowBackupManifest.load(
                from: prepared.fixture.persistence.legacyModelChapterWorkflowBackupRootURL,
                sourceGeneration: prepared.snapshot.sourceGeneration
            ),
            prepared.snapshot.backup
        )
    }

    func testBootstrapCommitsWhenProcessDiedAfterLegacyDeletion() throws {
        let prepared = try prepareInterruptedCutover()
        defer { SharedTranscriptRecoveryTestSupport.dispose(prepared.fixture) }
        try prepared.snapshot.backup.publish(
            to: prepared.fixture.persistence.legacyModelChapterWorkflowBackupRootURL
        )
        try prepared.jobs.removeJobs(kind: .chapterArtifacts)

        let client = try bootstrap(prepared.fixture)
        defer { client.shutdown() }

        assertAuthoritativeAmbiguity(client, episodeID: prepared.fixture.episodeID)
        XCTAssertTrue(try prepared.jobs.allJobs().allSatisfy { $0.kind != .chapterArtifacts })
    }

    func testBootstrapFailsClosedWhenRowsDisappearWithoutVerifiedBackup() throws {
        let prepared = try prepareInterruptedCutover()
        defer { SharedTranscriptRecoveryTestSupport.dispose(prepared.fixture) }
        try prepared.jobs.removeJobs(kind: .chapterArtifacts)

        switch SharedLibraryBootstrap.run(
            persistence: prepared.fixture.persistence,
            feedHost: QueuedCoreFeedHost([]),
            chapterCompilationModel: configuredModel
        ) {
        case .ready(let client):
            client.shutdown()
            XCTFail("Deleted rows without a backup must not commit authority")
        case .authoritativeUnavailable(let reason, let stage):
            XCTAssertEqual(reason, SharedLibraryBootstrapFailureCode.verificationFailed.rawValue)
            XCTAssertEqual(stage, .modelChapterWorkflowCutover)
        }

        let facade = try Pod0Facade.open(
            storePath: prepared.fixture.persistence.sharedCoreStoreURL.path
        )
        XCTAssertEqual(facade.modelChapterCutover().stage, .staged)
    }

    func testBootstrapFailsClosedWhenBackupIsCorruptAfterLegacyDeletion() throws {
        let prepared = try prepareInterruptedCutover()
        defer { SharedTranscriptRecoveryTestSupport.dispose(prepared.fixture) }
        let root = prepared.fixture.persistence.legacyModelChapterWorkflowBackupRootURL
        try prepared.snapshot.backup.publish(to: root)
        let backupURL = try XCTUnwrap(FileManager.default.contentsOfDirectory(
            at: root,
            includingPropertiesForKeys: nil
        ).first { $0.lastPathComponent.contains("\(prepared.snapshot.sourceGeneration)-") })
        try Data("corrupt".utf8).write(to: backupURL, options: .atomic)
        try prepared.jobs.removeJobs(kind: .chapterArtifacts)

        switch SharedLibraryBootstrap.run(
            persistence: prepared.fixture.persistence,
            feedHost: QueuedCoreFeedHost([]),
            chapterCompilationModel: configuredModel
        ) {
        case .ready(let client):
            client.shutdown()
            XCTFail("Corrupt rollback evidence must not commit authority")
        case .authoritativeUnavailable(let reason, let stage):
            XCTAssertEqual(reason, SharedLibraryBootstrapFailureCode.verificationFailed.rawValue)
            XCTAssertEqual(stage, .modelChapterWorkflowCutover)
        }
        let facade = try Pod0Facade.open(
            storePath: prepared.fixture.persistence.sharedCoreStoreURL.path
        )
        XCTAssertEqual(facade.modelChapterCutover().stage, .staged)
    }

    func testBootstrapRestagesWhenLegacyRowsChangeAfterStaging() throws {
        let prepared = try prepareInterruptedCutover()
        defer { SharedTranscriptRecoveryTestSupport.dispose(prepared.fixture) }
        try insert(
            prepared.jobs,
            episodeID: UUID(),
            key: "late-legacy-model-job",
            inputVersion: "late-source-v1"
        )
        let refreshed = try LegacyModelChapterWorkflowSnapshot.capture(from: prepared.jobs)

        let client = try bootstrap(prepared.fixture)
        defer { client.shutdown() }

        assertAuthoritativeAmbiguity(client, episodeID: prepared.fixture.episodeID)
        XCTAssertEqual(
            client.facade.modelChapterCutover().sourceGeneration,
            refreshed.sourceGeneration
        )
        XCTAssertTrue(try prepared.jobs.allJobs().allSatisfy { $0.kind != .chapterArtifacts })
        XCTAssertEqual(try LegacyModelChapterWorkflowBackupManifest.load(
            from: prepared.fixture.persistence.legacyModelChapterWorkflowBackupRootURL,
            sourceGeneration: refreshed.sourceGeneration
        ), refreshed.backup)
    }

    func testBootstrapRecoversWhenProcessDiesAfterStagedDiscard() throws {
        let prepared = try prepareInterruptedCutover()
        defer { SharedTranscriptRecoveryTestSupport.dispose(prepared.fixture) }
        var facade: Pod0Facade? = try Pod0Facade.open(
            storePath: prepared.fixture.persistence.sharedCoreStoreURL.path
        )
        let discarded = try XCTUnwrap(facade).discardStagedLegacyModelChapterCutover(
            sourceGeneration: prepared.snapshot.sourceGeneration
        )
        XCTAssertEqual(discarded.stage, .notStarted)
        facade = nil

        let client = try bootstrap(prepared.fixture)
        defer { client.shutdown() }
        assertAuthoritativeAmbiguity(client, episodeID: prepared.fixture.episodeID)
        XCTAssertTrue(try prepared.jobs.allJobs().allSatisfy { $0.kind != .chapterArtifacts })
    }

    private struct PreparedCutover {
        let fixture: SharedTranscriptRecoveryTestSupport.Fixture
        let jobs: JobStore
        let snapshot: LegacyModelChapterWorkflowSnapshot
    }

    private func prepareInterruptedCutover() throws -> PreparedCutover {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        do {
            try SharedTranscriptRecoveryTestSupport.seedLegacyTranscript(fixture)
            let initial = try bootstrap(fixture)
            guard case .ready(let request) = initial.chapterModelPlan(
                episodeID: fixture.episodeID,
                configuredModel: configuredModel
            ) else {
                throw TestFailure.modelPlanUnavailable
            }
            initial.shutdown()

            try resetModelAuthority(at: fixture.persistence.sharedCoreStoreURL)
            let jobs = JobStore(fileURL: fixture.persistence.episodeStore.fileURL)
            let running = try claim(
                jobs,
                episodeID: fixture.episodeID,
                key: "legacy-model-running",
                inputVersion: request.sourceVersion
            )
            try jobs.markRunning(
                id: running.id,
                leaseToken: try XCTUnwrap(running.leaseToken)
            )
            var interruptedFacade: Pod0Facade? = try Pod0Facade.open(
                storePath: fixture.persistence.sharedCoreStoreURL.path
            )
            let snapshot = try LegacyModelChapterWorkflowSnapshot.capture(from: jobs)
            let staged = try XCTUnwrap(interruptedFacade).stageLegacyModelChapterCutover(
                sourceGeneration: snapshot.sourceGeneration,
                configuredModel: configuredModel,
                candidates: snapshot.candidates
            )
            XCTAssertEqual(staged.stage, .staged)
            XCTAssertEqual(staged.adoptedAmbiguous, 1)
            XCTAssertTrue(try XCTUnwrap(interruptedFacade).nextHostRequests(
                maximumCount: 64
            ).isEmpty)
            interruptedFacade = nil
            return PreparedCutover(fixture: fixture, jobs: jobs, snapshot: snapshot)
        } catch {
            SharedTranscriptRecoveryTestSupport.dispose(fixture)
            throw error
        }
    }

    private func bootstrap(
        _ fixture: SharedTranscriptRecoveryTestSupport.Fixture
    ) throws -> SharedLibraryClient {
        switch SharedLibraryBootstrap.run(
            persistence: fixture.persistence,
            feedHost: QueuedCoreFeedHost([]),
            chapterCompilationModel: configuredModel
        ) {
        case .ready(let client): client
        case .authoritativeUnavailable(let reason, let stage):
            throw TestFailure.bootstrapFailed("\(stage.rawValue):\(reason)")
        }
    }

    private func assertAuthoritativeAmbiguity(
        _ client: SharedLibraryClient,
        episodeID: UUID,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        XCTAssertEqual(client.facade.modelChapterCutover().stage, .authoritative, file: file, line: line)
        let envelope = client.facade.snapshot(request: ProjectionRequest(
            scope: .chapterWorkflows(episodeId: EpisodeId(uuid: episodeID)),
            offset: 0,
            maxItems: 2
        ))
        guard case .chapterWorkflows(let projection) = envelope.projection else {
            return XCTFail("Expected chapter workflow projection", file: file, line: line)
        }
        XCTAssertEqual(projection.model.first?.stage, .ambiguous, file: file, line: line)
        XCTAssertTrue(client.facade.nextHostRequests(maximumCount: 64).isEmpty, file: file, line: line)
    }

    private func resetModelAuthority(at fileURL: URL) throws {
        try WorkflowSQLite.withDatabase(fileURL: fileURL) { db in
            try WorkflowSQLite.execute("BEGIN IMMEDIATE TRANSACTION", db)
            do {
                try WorkflowSQLite.execute("DELETE FROM pod0_model_chapter_completions", db)
                try WorkflowSQLite.execute("DELETE FROM pod0_model_chapter_workflows", db)
                try WorkflowSQLite.execute(
                    "DELETE FROM pod0_domain_cutovers WHERE domain='model_chapter_workflows'",
                    db
                )
                try WorkflowSQLite.execute("COMMIT TRANSACTION", db)
            } catch {
                try? WorkflowSQLite.execute("ROLLBACK TRANSACTION", db)
                throw error
            }
        }
    }

    private func insert(
        _ store: JobStore,
        episodeID: UUID,
        key: String,
        inputVersion: String
    ) throws {
        _ = try store.ensureJob(DesiredJob(
            idempotencyKey: key,
            kind: .chapterArtifacts,
            subjectID: episodeID,
            inputVersion: inputVersion,
            resourceClass: .utilityLLM
        ), notBefore: .distantPast)
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
            owner: "cutover-test",
            leaseDuration: 60
        ).first)
    }

    private enum TestFailure: Error {
        case bootstrapFailed(String)
        case modelPlanUnavailable
    }
}
