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
        let locations = try persistence.userDataErasureLocations()
        let client = fenceNativeStateForUserDataErasure()
        await persistence.fenceForUserDataErasure()
        await CostLedger.shared.fenceForUserDataErasure()
        let signalStore = productSignals as? ProductSignalStore
        await signalStore?.fenceForUserDataErasure()
        let initial: UserDataErasureResult
        if let pending = try recoverPendingErasure(locations: locations) {
            initial = pending
        } else {
            guard let client else {
                throw SharedLibraryError.unavailable
            }
            let expectedStoreID = try client.facade.storeIdentity()
            let token = try client.facade.prepareErasure(
                expectedStoreId: expectedStoreID,
                nonce: Self.erasureNonce(),
                retainedSettingsJson: try Self.encodeRetainedSettings(settings),
                locations: locations
            )
            initial = try client.facade.confirmErasure(token: token)
        }
        let freshStoreID = try await NativeUserDataErasureExecutor.finish(
            initial,
            locations: locations
        )
        await persistence.resumeAfterUserDataErasure()
        let freshState = try persistence.load()
        let outcome = SharedLibraryBootstrap.run(
            persistence: persistence,
            legacyState: freshState
        )
        guard case .ready(let freshClient) = outcome,
              try freshClient.facade.storeIdentity() == freshStoreID
        else {
            throw SharedLibraryError.unavailable
        }
        await signalStore?.resumeAfterUserDataErasure()
        installFreshStateAfterUserDataErasure(freshState, client: freshClient)
    }

    private static func encodeRetainedSettings(_ settings: Settings?) throws -> Data {
        guard let settings else { return Data("{}".utf8) }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(settings)
    }

    private static func erasureNonce() -> Data {
        var first = UUID().uuid
        var second = UUID().uuid
        var data = withUnsafeBytes(of: &first) { Data($0) }
        data.append(withUnsafeBytes(of: &second) { Data($0) })
        return data
    }
}
