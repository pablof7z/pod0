import Foundation

extension AppStateStore {
    /// Deletes Pod0 product data while retaining settings and every Keychain
    /// item. NMP store and receipt-annotation policy are owned by the caller.
    func clearAllData() {
        resetProductState(preserving: state.settings)
    }

    /// Deletes the complete AppState projection for a local trust-domain handoff.
    /// Keychain, receipt annotations, and NMP store are reset separately so a
    /// failed prerequisite cannot silently turn this into a partial handoff.
    func clearAppStateForMutuallyUntrustedUser() {
        resetProductState(preserving: nil)
    }

    private func resetProductState(preserving settings: Settings?) {
        // Pending writes target episode ids that are about to disappear and
        // must not resurrect rows after the reset.
        positionFlushTask?.cancel()
        positionFlushTask = nil
        widgetReloadTask?.cancel()
        widgetReloadTask = nil
        positionCache.removeAll()

        performMutationBatch {
            mutateState {
                $0 = AppState()
                if let settings { $0.settings = settings }
            }
            invalidateEpisodeProjections()
        }
        SpotlightIndexer.clearAll()
    }
}
