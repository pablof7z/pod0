import Foundation
import Pod0Core
import os.log

struct AppStateStartupPreparation: @unchecked Sendable {
    let state: AppState
    let loadFailed: Bool
    let bootstrap: SharedLibraryBootstrapPreparationOutcome?
    let needsNativeProjectionRetirement: Bool
    let needsRecallConfigurationRetirement: Bool
}

enum AppStateStartupPreparer {
    private static let logger = Logger.app("AppStateStartupPreparer")

    static func prepare(
        persistence: Persistence,
        sharedFeedHost: (any CoreFeedHosting)?
    ) -> AppStateStartupPreparation {
        var loadedState: AppState
        do {
            let chapterAuthorityActive = FileManager.default.fileExists(
                atPath: persistence.sharedCoreStoreURL.path
            ) && sharedChapterStoreIsAuthoritative(
                targetPath: persistence.sharedCoreStoreURL.path
            )
            loadedState = try persistence.load(
                loadLegacyChapterAdjuncts: !chapterAuthorityActive
            )
        } catch {
            logger.error(
                "Persistence.load failed; startup is blocked and persisted data is untouched"
            )
            return AppStateStartupPreparation(
                state: AppState(),
                loadFailed: true,
                bootstrap: nil,
                needsNativeProjectionRetirement: false,
                needsRecallConfigurationRetirement: false
            )
        }

        AppStateStore.migrateLegacyOpenRouterSecretIfNeeded(
            in: &loadedState,
            persistence: persistence
        )
        EpisodeShowNotesFormatter.prewarm(
            loadedState.episodes.prefix(1_000).map(\.description)
        )
        removeLegacyExternalPodcasts(from: &loadedState)
        seedLegacyImportRevisionIfNeeded(in: &loadedState, persistence: persistence)
        let needsNativeRetirement = AppStateStore.hasMigratedNativeState(loadedState)
        let needsRecallRetirement =
            loadedState.settings.legacyRecallConfigurationSeed != nil
        let bootstrap = SharedLibraryBootstrap.prepare(
            persistence: persistence,
            legacyState: loadedState,
            feedHost: sharedFeedHost ?? CoreFeedHost(),
            legacyRecallConfiguration: loadedState.settings.legacyRecallConfigurationSeed
        )
        return AppStateStartupPreparation(
            state: loadedState,
            loadFailed: false,
            bootstrap: bootstrap,
            needsNativeProjectionRetirement: needsNativeRetirement,
            needsRecallConfigurationRetirement: needsRecallRetirement
        )
    }

    private static func removeLegacyExternalPodcasts(from state: inout AppState) {
        let podcastIDs = Set(
            state.podcasts
                .filter { $0.feedURL?.scheme == "external-episode" }
                .map(\.id)
        )
        guard !podcastIDs.isEmpty else { return }
        state.podcasts.removeAll { podcastIDs.contains($0.id) }
        state.subscriptions.removeAll { podcastIDs.contains($0.podcastID) }
    }

    private static func seedLegacyImportRevisionIfNeeded(
        in state: inout AppState,
        persistence: Persistence
    ) {
        guard !FileManager.default.fileExists(
            atPath: persistence.sharedCoreStoreURL.path
        ) else { return }
        let nextGeneration = state.persistenceGeneration == .max
            ? UInt64.max
            : state.persistenceGeneration + 1
        let importRevision = max(nextGeneration, 1)
        state.persistenceGeneration = importRevision
        _ = persistence.write(state, revision: importRevision)
    }
}

extension AppStateStore {
    static func production(
        productSignals: any ProductSignalSink = ProductSignalStore.shared
    ) async -> AppStateStore {
        let persistence = Persistence.shared
        let preparation = await Task.detached(priority: .userInitiated) {
            AppStateStartupPreparer.prepare(
                persistence: persistence,
                sharedFeedHost: nil
            )
        }.value
        return AppStateStore(
            preparedStartup: preparation,
            persistence: persistence,
            productSignals: productSignals,
            startSubscriptionRefresh: true
        )
    }
}
