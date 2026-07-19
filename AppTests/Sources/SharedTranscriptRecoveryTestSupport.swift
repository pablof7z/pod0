import CryptoKit
import Foundation
import CSQLiteVec
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
enum SharedTranscriptRecoveryTestSupport {
    struct Fixture {
        let persistence: Persistence
        let fileURL: URL
        let podcastID: UUID
        let episodeID: UUID
    }

    static func makeFixture() -> Fixture {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        let podcastID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
        let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
        var state = AppState()
        state.podcasts = [Podcast(
            id: podcastID,
            feedURL: URL(string: "https://recovery.example/feed.xml")!,
            title: "Transcript Recovery"
        )]
        state.subscriptions = [PodcastSubscription(podcastID: podcastID)]
        state.episodes = [Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: "transcript-recovery",
            title: "Transcript Recovery",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://recovery.example/episode.mp3")!
        )]
        XCTAssertTrue(persistence.write(state, revision: 1))
        return Fixture(
            persistence: persistence,
            fileURL: fileURL,
            podcastID: podcastID,
            episodeID: episodeID
        )
    }

    static func dispose(_ fixture: Fixture) {
        AppStateTestSupport.disposeIsolatedStore(at: fixture.fileURL)
    }

    static func transcript(episodeID: UUID, revision: String) -> Transcript {
        Transcript(
            id: UUID(uuidString: revision == "audio-v1"
                ? "33333333-3333-3333-3333-333333333333"
                : "33333333-3333-3333-3333-333333333334")!,
            episodeID: episodeID,
            language: "en-US",
            source: .publisher,
            segments: [Segment(
                id: UUID(uuidString: "44444444-4444-4444-4444-444444444444")!,
                start: 1,
                end: 3,
                text: revision == "audio-v1" ? "Original durable transcript" : "Replacement transcript"
            )],
            generatedAt: Date(timeIntervalSince1970: 1_800_000_000)
        )
    }

    static func context(podcastID: UUID, revision: String) -> TranscriptObservationContext {
        TranscriptObservationContext(
            podcastID: podcastID,
            sourceRevision: revision,
            sourcePayloadDigest: ArtifactRepository.hash(Data("payload-\(revision)".utf8)),
            provider: "publisher-feed"
        )
    }

    @discardableResult
    static func seedLegacyTranscript(_ fixture: Fixture) throws -> Data {
        let transcript = transcript(episodeID: fixture.episodeID, revision: "audio-v1")
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let bytes = try encoder.encode(transcript)
        let root = fixture.persistence.legacyTranscriptRootURL
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try bytes.write(
            to: root.appendingPathComponent("\(fixture.episodeID.uuidString).json"),
            options: .atomic
        )
        try WorkflowSQLite.withDatabase(fileURL: fixture.persistence.episodeStore.fileURL) { db in
            try WorkflowSchemaMigrations.ensureArtifacts(db)
            let statement = try WorkflowSQLite.prepare(
                """
                INSERT INTO artifacts(
                    kind,subject_id,input_version,output_version,content_hash,
                    location,origin,schema_version,integrity,verified_at,selected
                ) VALUES('transcript',?,?,?,?,NULL,'publisher',1,'available',?,1)
                """,
                db: db
            )
            defer { sqlite3_finalize(statement) }
            try WorkflowSQLite.bind(fixture.episodeID.uuidString, 1, statement, db)
            try WorkflowSQLite.bind("audio-v1", 2, statement, db)
            try WorkflowSQLite.bind("legacy-output-v1", 3, statement, db)
            try WorkflowSQLite.bind(ArtifactRepository.hash(bytes), 4, statement, db)
            try WorkflowSQLite.bind(Date(timeIntervalSince1970: 1_800_000_000), 5, statement, db)
            try WorkflowSQLite.stepDone(statement, db)
        }
        return bytes
    }

    static func injectLegacyReadyState(_ fixture: Fixture) throws {
        try WorkflowSQLite.withDatabase(fileURL: fixture.persistence.episodeStore.fileURL) { db in
            let read = try WorkflowSQLite.prepare(
                "SELECT payload FROM episodes WHERE id=?",
                db: db
            )
            defer { sqlite3_finalize(read) }
            try WorkflowSQLite.bind(fixture.episodeID.uuidString, 1, read, db)
            guard sqlite3_step(read) == SQLITE_ROW,
                  let payload = WorkflowSQLite.data(read, 0),
                  var object = try JSONSerialization.jsonObject(with: payload) as? [String: Any]
            else { return XCTFail("Missing legacy episode payload") }
            object["transcriptState"] = ["ready": ["source": "publisher"]]
            let updated = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
            let write = try WorkflowSQLite.prepare(
                "UPDATE episodes SET payload=? WHERE id=?",
                db: db
            )
            defer { sqlite3_finalize(write) }
            try WorkflowSQLite.bind(updated, 1, write, db)
            try WorkflowSQLite.bind(fixture.episodeID.uuidString, 2, write, db)
            try WorkflowSQLite.stepDone(write, db)
        }
    }

    static func prepareCorePrerequisites(_ fixture: Fixture) throws -> CommandId {
        let target = fixture.persistence.sharedCoreStoreURL
        let observedAt = Int64(1_700_000_000_000)
        let storeID = stableID("pod0-core-store:\(target.standardizedFileURL.path)")
        let schemaBackup = fixture.persistence.sharedCoreSchemaBackupURL(
            targetVersion: sharedStoreSchemaVersion()
        )
        _ = try prepareSharedListeningStore(
            targetPath: target.path,
            schemaBackupPath: schemaBackup.path,
            migrationId: storeID,
            observedAtMilliseconds: observedAt
        )
        do {
            _ = try commitStagedLegacyListeningImport(
                targetPath: target.path,
                observedAtMilliseconds: observedAt
            )
        } catch LegacyListeningMigrationError.ImportNotFound {
            let plan = try inspectLegacyListeningSource(
                sourcePath: fixture.persistence.episodeStore.fileURL.path
            )
            let importID = CommandId(high: 90, low: 2)
            _ = try stageLegacyListeningImport(
                sourcePath: fixture.persistence.episodeStore.fileURL.path,
                sourceBackupPath: fixture.persistence.legacyListeningBackupURL.path,
                targetPath: target.path,
                targetSchemaBackupPath: schemaBackup.path,
                expectedPlan: plan,
                importId: importID,
                targetStoreId: storeID,
                observedAtMilliseconds: observedAt
            )
            _ = try readStagedLegacyListeningImport(
                targetPath: target.path,
                importId: importID
            )
            _ = try commitStagedLegacyListeningImport(
                targetPath: target.path,
                observedAtMilliseconds: observedAt
            )
        }
        try prepareNotes(fixture, target: target, schemaBackup: schemaBackup, storeID: storeID)
        try prepareClips(fixture, target: target, schemaBackup: schemaBackup, storeID: storeID)
        return storeID
    }

    private static func prepareNotes(
        _ fixture: Fixture,
        target: URL,
        schemaBackup: URL,
        storeID: CommandId
    ) throws {
        let observedAt = Int64(1_700_000_000_010)
        do {
            _ = try commitStagedLegacyNoteImport(
                targetPath: target.path,
                observedAtMilliseconds: observedAt
            )
        } catch LegacyNoteMigrationError.ImportNotFound {
            let plan = try inspectLegacyNoteSource(sourcePath: fixture.persistence.episodeStore.fileURL.path)
            let importID = stableID("test-note-import:\(plan.sourceHash):\(plan.sourceGeneration)")
            _ = try stageLegacyNoteImport(
                sourcePath: fixture.persistence.episodeStore.fileURL.path,
                sourceBackupPath: fixture.persistence.legacyNotesBackupURL.path,
                targetPath: target.path,
                targetSchemaBackupPath: schemaBackup.path,
                expectedPlan: plan,
                importId: importID,
                targetStoreId: storeID,
                observedAtMilliseconds: observedAt
            )
            _ = try readStagedLegacyNoteImport(targetPath: target.path, importId: importID)
            _ = try commitStagedLegacyNoteImport(
                targetPath: target.path,
                observedAtMilliseconds: observedAt
            )
        }
    }

    private static func prepareClips(
        _ fixture: Fixture,
        target: URL,
        schemaBackup: URL,
        storeID: CommandId
    ) throws {
        let observedAt = Int64(1_700_000_000_020)
        do {
            _ = try commitStagedLegacyClipImport(
                sourcePath: fixture.persistence.episodeStore.fileURL.path,
                targetPath: target.path,
                observedAtMilliseconds: observedAt
            )
        } catch LegacyClipMigrationError.ImportNotFound {
            let plan = try inspectLegacyClipSource(sourcePath: fixture.persistence.episodeStore.fileURL.path)
            let importID = stableID("test-clip-import:\(plan.sourceHash):\(plan.sourceGeneration)")
            _ = try stageLegacyClipImport(
                sourcePath: fixture.persistence.episodeStore.fileURL.path,
                sourceBackupPath: fixture.persistence.legacyClipsBackupURL(for: plan).path,
                targetPath: target.path,
                targetSchemaBackupPath: schemaBackup.path,
                expectedPlan: plan,
                importId: importID,
                targetStoreId: storeID,
                observedAtMilliseconds: observedAt
            )
            _ = try readStagedLegacyClipImport(targetPath: target.path, importId: importID)
            _ = try commitStagedLegacyClipImport(
                sourcePath: fixture.persistence.episodeStore.fileURL.path,
                targetPath: target.path,
                observedAtMilliseconds: observedAt
            )
        }
    }

    private static func stableID(_ seed: String) -> CommandId {
        let digest = Array(SHA256.hash(data: Data(seed.utf8)))
        let high = digest[0..<8].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        let low = digest[8..<16].reduce(UInt64(0)) { ($0 << 8) | UInt64($1) }
        return CommandId(high: high, low: low)
    }
}
