import Foundation

#if canImport(NMP)
import NMP

extension UserIdentityStore {
    /// Binds the UI projection to the sole process-lifetime NMP composition,
    /// restores its clean-start human entry. New identity generation remains
    /// fail-closed until NMP exposes that capability.
    func start(
        composition: Pod0NMPComposition,
        catalogStorage: any Pod0IdentityCatalogStorage = KeychainPod0IdentityCatalogStorage()
    ) async {
        guard nmpComposition == nil else { return }
        nmpComposition = composition
        let lifecycle = Pod0HumanIdentityLifecycle(
            engineAccess: composition,
            catalogStorage: catalogStorage
        )
        nmpLifecycle = lifecycle
        do {
            if let entry = try await lifecycle.restoreHuman() {
                adoptNMPIdentity(entry: entry)
            } else {
                failIdentity(UserIdentityError.nmpKeyGenerationUnavailable)
            }
        } catch {
            failIdentity(error)
        }
    }

    func importNsec(_ nsec: String) async throws {
        loginError = nil
        guard let lifecycle = nmpLifecycle else { throw UserIdentityError.nmpUnavailable }
        do {
            let entry = try await lifecycle.registerLocal(
                secret: nsec.trimmed,
                origin: .importedNsec,
                label: "My account"
            )
            adoptNMPIdentity(entry: entry)
        } catch {
            loginError = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            throw error
        }
    }

    func clearIdentity() {
        do {
            try nmpLifecycle?.cachePreservingSignOut()
            loginError = nil
            clearPublishedState()
        } catch {
            loginError = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
            logger.error("NMP identity sign-out failed: \(String(describing: error), privacy: .public)")
        }
    }

    func connectRemoteSigner(uri: String) async {
        loginError = nil
        remoteSignerState = .connecting
        guard let lifecycle = nmpLifecycle else {
            failIdentity(UserIdentityError.nmpUnavailable)
            return
        }
        do {
            let entry = try await lifecycle.connectBunker(uri: uri.trimmed)
            adoptNMPIdentity(entry: entry)
        } catch {
            failIdentity(error)
        }
    }

    func disconnectRemoteSigner() async {
        clearIdentity()
    }

    func failIdentity(_ error: Error) {
        let message = (error as? LocalizedError)?.errorDescription ?? String(describing: error)
        loginError = message
        remoteSignerState = .failed(message)
    }

    private func adoptNMPIdentity(entry: Pod0IdentityCatalogEntry) {
        publicKeyHex = entry.expectedPublicKey
        mode = switch entry.capability {
        case .localKey: .localKey
        case .nip46Bunker: .remoteSigner
        }
        remoteSignerState = mode == .remoteSigner
            ? .connected(entry.expectedPublicKey)
            : .idle
    }

}
#endif
