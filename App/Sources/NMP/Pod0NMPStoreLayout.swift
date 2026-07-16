import Foundation

struct Pod0NMPStoreLayout: Sendable, Hashable {
    enum BackupPolicy: String, Sendable, Codable {
        /// Canonical facts are re-acquired from NMP; identity secrets live in
        /// Keychain. The engine store and quarantine ledger stay out of device
        /// backups so a restore cannot combine stale protocol facts with a new
        /// device trust domain.
        case excludedFromDeviceBackup
    }

    let rootDirectory: URL
    let storeURL: URL
    let migrationRecordURL: URL
    let quarantineArchiveURL: URL
    let backupPolicy: BackupPolicy

    init(rootDirectory: URL, backupPolicy: BackupPolicy = .excludedFromDeviceBackup) {
        self.rootDirectory = rootDirectory.standardizedFileURL
        storeURL = self.rootDirectory.appendingPathComponent("canonical.redb", isDirectory: false)
        migrationRecordURL = self.rootDirectory.appendingPathComponent("migration-v1.json", isDirectory: false)
        quarantineArchiveURL = self.rootDirectory.appendingPathComponent("legacy-quarantine-v1.json", isDirectory: false)
        self.backupPolicy = backupPolicy
    }

    static func applicationSupport(fileManager: FileManager = .default) throws -> Pod0NMPStoreLayout {
        let support = try fileManager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        return Pod0NMPStoreLayout(
            rootDirectory: support
                .appendingPathComponent("podcastr", isDirectory: true)
                .appendingPathComponent("nmp", isDirectory: true)
        )
    }

    func prepare(fileManager: FileManager = .default) throws {
        try fileManager.createDirectory(at: rootDirectory, withIntermediateDirectories: true)
        guard backupPolicy == .excludedFromDeviceBackup else { return }
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var mutableRoot = rootDirectory
        try mutableRoot.setResourceValues(values)
    }
}

