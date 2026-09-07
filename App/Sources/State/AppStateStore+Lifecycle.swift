import Foundation
import Pod0Core

extension AppStateStore {
    nonisolated static func cleanupOrphanedWikiFilesIfNeeded() {
        let flagKey = "cleanup.wikiFilesRemoved.v1"
        guard !UserDefaults.standard.bool(forKey: flagKey) else { return }
        defer { UserDefaults.standard.set(true, forKey: flagKey) }
        let manager = FileManager.default
        guard let base = try? manager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: false
        ) else { return }
        let wikiDirectory = base
            .appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("wiki", isDirectory: true)
        try? manager.removeItem(at: wikiDirectory)
    }

    func applyExternalSettingsChange() {
        guard syncSettingsWithICloud,
              let client = sharedLibrary,
              let current = productSettingsProjection
        else { return }
        let sync = iCloudSettingsSync.shared
        do {
            guard let remote = try sync.remoteSnapshot(basedOn: state.settings) else {
                sync.push(current)
                return
            }
            let projection = try client.facade.mergeRemoteProductSettings(
                commandId: CommandId(uuid: UUID()),
                schemaVersion: remote.schemaVersion,
                writerVersion: remote.writerVersion,
                values: remote.values
            )
            guard projection.authoritative, let committed = projection.settings else { return }
            productSettingsProjection = committed
            let updated = ProductSettingsBridge.applying(committed.values, to: state.settings)
            if updated != state.settings {
                Self.logger.info("iCloudSettingsSync: applying Rust-merged settings projection")
                mutateProjectionState { $0.settings = updated }
                WorkflowRuntime.shared.wake()
            }
            sync.push(committed)
        } catch {
            Self.logger.error("iCloud settings observation was rejected by the shared core")
        }
    }

    nonisolated static func migrateLegacyOpenRouterSecretIfNeeded(
        in state: inout AppState,
        persistence: Persistence,
        saveCredential: (String) throws -> Void = OpenRouterCredentialStore.saveAPIKey,
        readCredential: () throws -> String? = OpenRouterCredentialStore.apiKey
    ) {
        let legacyKey = state.settings.legacyOpenRouterAPIKey.trimmedOrEmpty
        guard !legacyKey.isEmpty else {
            state.settings.legacyOpenRouterAPIKey = nil
            return
        }
        do {
            try saveCredential(legacyKey)
            guard try readCredential()?.trimmed == legacyKey else {
                throw LegacyOpenRouterMigrationError.readBackMismatch
            }
            state.settings.markOpenRouterManual()
            persistence.save(state)
        } catch {
            logger.error(
                "Legacy OpenRouter key remains at its source because Keychain migration was not verified"
            )
        }
    }

    func updateSettings(_ settings: Settings) {
        guard let client = sharedLibrary, let current = productSettingsProjection else {
            Self.logger.error("Blocked settings mutation before Rust authority")
            return
        }
        do {
            let projection = try client.facade.setProductSettings(
                commandId: CommandId(uuid: UUID()),
                expectedRevision: current.revision,
                writerId: ProductSettingsWriterIdentity.current(),
                values: try ProductSettingsBridge.values(from: settings)
            )
            guard projection.authoritative, let committed = projection.settings else { return }
            productSettingsProjection = committed
            let projected = ProductSettingsBridge.applying(committed.values, to: settings)
            mutateState { $0.settings = projected }
            if syncSettingsWithICloud {
                iCloudSettingsSync.shared.push(committed)
            }
            WorkflowRuntime.shared.wake()
        } catch {
            Self.logger.error("Settings mutation was rejected by the shared core")
        }
    }

    /// Captures the latest unmigrated native state and awaits its authoritative
    /// SQLite commit before iOS suspends the process.
    func flushForSuspension() async -> Bool {
        guard !startupRecoveryRequired, sharedLibraryUnavailableReason == nil else {
            Self.logger.error("Skipped suspension flush while persistence recovery is required")
            return false
        }
        return await persistence.flush(state)
    }
}

private enum LegacyOpenRouterMigrationError: Error {
    case readBackMismatch
}
