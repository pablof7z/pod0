import Foundation
import Pod0Core

extension AppStateStore {
    /// Deletes Pod0 product data while retaining settings and every Keychain item.
    func clearAllData() {
        let settings = state.settings
        Task { @MainActor [weak self] in
            do {
                try await self?.resetProductState(preserving: settings)
            } catch {
                Self.logger.error(
                    "Product reset failed: \(error.localizedDescription, privacy: .public)"
                )
            }
        }
    }

    /// Awaitable reset boundary used by destructive-flow qualification.
    func clearAllDataAndWait() async throws {
        try await resetProductState(preserving: state.settings)
    }

    /// Deletes the complete AppState projection for a local trust-domain handoff.
    func clearAppStateForMutuallyUntrustedUser() {
        Task { @MainActor [weak self] in
            do {
                try await self?.resetProductState(preserving: nil)
            } catch {
                Self.logger.error(
                    "Trust-domain reset failed: \(error.localizedDescription, privacy: .public)"
                )
            }
        }
    }

    private func resetProductState(preserving settings: Settings?) async throws {
        guard let sharedLibrary else {
            throw SharedLibraryError.unavailable
        }
        _ = try await sharedLibrary.execute(.resetListeningData)
        widgetReloadTask?.cancel()
        widgetReloadTask = nil

        performMutationBatch {
            mutateState {
                $0 = AppState()
                if let settings { $0.settings = settings }
            }
            invalidateEpisodeProjections()
        }
        await persistence.flush(state)
        SpotlightIndexer.clearAll()
        await productSignals.deleteAll()
    }
}
