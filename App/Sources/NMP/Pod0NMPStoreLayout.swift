import Foundation

struct Pod0NMPStoreLayout: Sendable, Hashable {
    enum BackupPolicy: String, Sendable, Codable {
        /// Canonical facts are re-acquired from NMP; identity secrets live in
        /// Keychain. The engine store stays out of device backups so a restore
        /// cannot combine stale protocol facts with a new device trust domain.
        case excludedFromDeviceBackup
    }

    enum FileProtectionPolicy: String, Sendable, Codable {
        /// The store is unavailable until the first device unlock after boot,
        /// then remains available for background NMP work while the device locks.
        case completeUntilFirstUserAuthentication
    }

    let rootDirectory: URL
    let storeURL: URL
    let backupPolicy: BackupPolicy
    let fileProtectionPolicy: FileProtectionPolicy

    init(
        rootDirectory: URL,
        backupPolicy: BackupPolicy = .excludedFromDeviceBackup,
        fileProtectionPolicy: FileProtectionPolicy = .completeUntilFirstUserAuthentication
    ) {
        self.rootDirectory = rootDirectory.standardizedFileURL
        storeURL = self.rootDirectory.appendingPathComponent("canonical.redb", isDirectory: false)
        self.backupPolicy = backupPolicy
        self.fileProtectionPolicy = fileProtectionPolicy
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
        if backupPolicy == .excludedFromDeviceBackup {
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            var mutableRoot = rootDirectory
            try mutableRoot.setResourceValues(values)
        }
        if fileProtectionPolicy == .completeUntilFirstUserAuthentication {
            try fileManager.setAttributes(
                [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                ofItemAtPath: rootDirectory.path
            )
        }
    }
}
