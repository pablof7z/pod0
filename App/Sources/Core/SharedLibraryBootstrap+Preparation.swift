import Foundation
import Pod0Core

struct SharedLibraryBootstrapPreparation: @unchecked Sendable {
    let facade: Pod0Facade
    let coreStoreURL: URL
    let feedHost: any CoreFeedHosting
    let observationOutbox: NativeHostObservationOutbox
}

enum SharedLibraryBootstrapPreparationOutcome: @unchecked Sendable {
    case ready(SharedLibraryBootstrapPreparation)
    case authoritativeUnavailable(reason: String, stage: SharedLibraryBootstrapStage)
}

extension SharedLibraryBootstrap {
    @MainActor
    static func run(
        persistence: Persistence,
        legacyState: AppState,
        feedHost: any CoreFeedHosting = CoreFeedHost(),
        legacyRecallConfiguration: LegacyRecallConfigurationSeed? = nil
    ) -> SharedLibraryBootstrapOutcome {
        finish(
            prepare(
                persistence: persistence,
                legacyState: legacyState,
                feedHost: feedHost,
                legacyRecallConfiguration: legacyRecallConfiguration
            ),
            persistence: persistence
        )
    }

    @MainActor
    static func finish(
        _ outcome: SharedLibraryBootstrapPreparationOutcome,
        persistence: Persistence
    ) -> SharedLibraryBootstrapOutcome {
        switch outcome {
        case .ready(let preparation):
            CoreDownloadHost.shared.configure(coreStoreURL: preparation.coreStoreURL)
            let client = SharedLibraryClient(
                facade: preparation.facade,
                coreStoreURL: preparation.coreStoreURL,
                feedHost: preparation.feedHost,
                downloadHost: CoreDownloadHost.shared,
                notificationHost: CoreNotificationHost(),
                observationOutbox: preparation.observationOutbox
            )
            client.start()
            persistence.activateSharedListeningAuthority()
            logger.info(
                "Shared Rust library is authoritative at \(preparation.coreStoreURL.path, privacy: .public)"
            )
            return .ready(client)
        case .authoritativeUnavailable(let reason, let stage):
            return .authoritativeUnavailable(reason: reason, stage: stage)
        }
    }
}
