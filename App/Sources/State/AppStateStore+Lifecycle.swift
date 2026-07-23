import Foundation

extension AppStateStore {
    static func cleanupOrphanedWikiFilesIfNeeded() {
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
        guard syncSettingsWithICloud else { return }
        let sync = iCloudSettingsSync.shared
        sync.isApplyingRemoteChange = true
        defer { sync.isApplyingRemoteChange = false }
        var updated = state.settings
        sync.merge(from: NSUbiquitousKeyValueStore.default, into: &updated)
        guard updated != state.settings else { return }
        Self.logger.info("iCloudSettingsSync: applying remote settings update")
        mutateState { $0.settings = updated }
    }

    static func migrateLegacyOpenRouterSecretIfNeeded(
        in state: inout AppState,
        persistence: Persistence
    ) {
        let legacyKey = state.settings.legacyOpenRouterAPIKey.trimmedOrEmpty
        guard !legacyKey.isEmpty else {
            state.settings.legacyOpenRouterAPIKey = nil
            return
        }
        do {
            try OpenRouterCredentialStore.saveAPIKey(legacyKey)
            state.settings.markOpenRouterManual()
        } catch {
            logger.error("Failed to migrate legacy OpenRouter key to keychain: \(error, privacy: .public)")
            state.settings.clearOpenRouterCredential()
        }
        persistence.save(state)
    }

    func updateSettings(_ settings: Settings) {
        let chapterModelChanged = settings.chapterCompilationModel
            != state.settings.chapterCompilationModel
        mutateState { $0.settings = settings }
        if chapterModelChanged { WorkflowRuntime.shared.wake() }
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
