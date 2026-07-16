import CryptoKit
import Foundation

enum NMPInstalledStateMigrationError: Error, Equatable {
    case unsupportedSchema(Int)
    case sourceStateChanged(expected: String, actual: String)
    case phaseRegression(from: NMPMigrationRecordV1.Phase, to: NMPMigrationRecordV1.Phase)
    case rollbackAfterCutover
}

struct NMPInstalledStateMigrationResult: Sendable {
    let activeState: AppState
    let record: NMPMigrationRecordV1
    let didPrepare: Bool
}

/// App-owned, atomically persisted migration ledger. Legacy Nostr facts move
/// only into a read-only quarantine file; no NMP ingest API is called because
/// upstream intentionally exposes no historical-import door.
struct NMPInstalledStateMigration {
    let layout: Pod0NMPStoreLayout
    let fileManager: FileManager

    init(layout: Pod0NMPStoreLayout, fileManager: FileManager = .default) {
        self.layout = layout
        self.fileManager = fileManager
    }

    func prepareIfNeeded(
        state: AppState,
        expectedPublicKeys: [Pod0IdentityRole: String],
        nmpRevision: String = Pod0NMPBuild.testedRevision,
        now: Date = Date()
    ) throws -> NMPInstalledStateMigrationResult {
        try layout.prepare(fileManager: fileManager)
        if let existing = try loadRecord() {
            return NMPInstalledStateMigrationResult(
                activeState: Self.quarantiningProtocolAuthority(in: state),
                record: existing,
                didPrepare: false
            )
        }

        let redacted = DataExport.redactedState(from: state)
        let sourceDigest = try Self.digest(redacted)
        let archive = LegacyNostrQuarantineV1(state: state, now: now)
        let archiveData = try Self.encoder.encode(archive)
        let archiveDigest = Self.digest(archiveData)
        try archiveData.write(to: layout.quarantineArchiveURL, options: [.atomic])

        let record = NMPMigrationRecordV1(
            pinnedNMPCommit: nmpRevision,
            sourceStateDigest: sourceDigest,
            legacyArchiveDigest: archiveDigest,
            legacyArchivePath: layout.quarantineArchiveURL.path,
            nmpStoreGeneration: UUID(),
            nmpStorePath: layout.storeURL.path,
            expectedPublicKeysByRole: expectedPublicKeys,
            failClosedLegacyIngress: true,
            now: now
        )
        try writeRecord(record)
        return NMPInstalledStateMigrationResult(
            activeState: Self.quarantiningProtocolAuthority(in: state),
            record: record,
            didPrepare: true
        )
    }

    /// A same-source preparation is an explicit no-op; a different source is
    /// refused instead of silently replacing the audit chain.
    func verifySourceIsIdempotent(_ state: AppState) throws {
        guard let record = try loadRecord() else { return }
        let digest = try Self.digest(DataExport.redactedState(from: state))
        guard digest == record.sourceStateDigest else {
            throw NMPInstalledStateMigrationError.sourceStateChanged(
                expected: record.sourceStateDigest,
                actual: digest
            )
        }
    }

    @discardableResult
    func advance(to phase: NMPMigrationRecordV1.Phase, now: Date = Date()) throws -> NMPMigrationRecordV1? {
        guard var record = try loadRecord() else { return nil }
        guard record.schemaVersion == NMPMigrationRecordV1.schemaVersion else {
            throw NMPInstalledStateMigrationError.unsupportedSchema(record.schemaVersion)
        }
        if phase == record.phase { return record }
        guard phase > record.phase else {
            throw NMPInstalledStateMigrationError.phaseRegression(from: record.phase, to: phase)
        }
        record.phase = phase
        record.updatedAt = now
        try writeRecord(record)
        return record
    }

    func loadRecord() throws -> NMPMigrationRecordV1? {
        guard fileManager.fileExists(atPath: layout.migrationRecordURL.path) else { return nil }
        let record = try Self.decoder.decode(
            NMPMigrationRecordV1.self,
            from: Data(contentsOf: layout.migrationRecordURL)
        )
        guard record.schemaVersion == NMPMigrationRecordV1.schemaVersion else {
            throw NMPInstalledStateMigrationError.unsupportedSchema(record.schemaVersion)
        }
        return record
    }

    func rollbackBeforeCutover(resetStagedStore: () throws -> Void) throws {
        guard let record = try loadRecord() else { return }
        guard record.phase < .cutover else {
            throw NMPInstalledStateMigrationError.rollbackAfterCutover
        }
        try resetStagedStore()
        try? fileManager.removeItem(at: layout.migrationRecordURL)
        try? fileManager.removeItem(at: layout.quarantineArchiveURL)
    }

    private func writeRecord(_ record: NMPMigrationRecordV1) throws {
        let data = try Self.encoder.encode(record)
        try data.write(to: layout.migrationRecordURL, options: [.atomic])
    }

    private static func quarantiningProtocolAuthority(in state: AppState) -> AppState {
        var result = state
        result.nostrPendingApprovals = []
        result.pendingFriendMessages = []
        result.nostrConversations = []
        result.nostrProfileCache = [:]
        result.nostrRespondedEventIDs = []
        result.nostrSinceCursor = nil
        result.settings.nostrPublicRelays = []
        return result
    }

    private static func digest(_ state: AppState) throws -> String {
        digest(try encoder.encode(state))
    }

    private static func digest(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
