import Foundation

#if canImport(NMP)
import NMP
#endif

enum Pod0IdentityBlocker: Sendable, Codable, Equatable {
    case restoredLocalDetachUnsupported(issue: Int)
    case orphanedRestoredLocal(issue: Int)
    case identitySwitchUnsupported
    case expectedPublicKeyMismatch(expected: String, actual: String)
}

enum Pod0HumanIdentityState: Sendable, Equatable {
    case signedOut
    case registering
    case connecting
    case authorizationRequired(String)
    case ready(publicKey: String)
    case blocked(Pod0IdentityBlocker)
    case failed(String)
}

enum Pod0HumanIdentityError: LocalizedError, Equatable {
    case missingRestoredLocalAccount
    case wrongRole
    case expectedPublicKeyMismatch(expected: String, actual: String)
    case remoteConnectionEndedBeforeReady
    case remoteFailure(String)
    case rollbackFailed(original: String, rollback: String)
    case checkpointRecoveryFailed(String)
    case restoredLocalDetachUnsupported(issue: Int)
    case identitySwitchUnsupported

    var errorDescription: String? {
        switch self {
        case .missingRestoredLocalAccount:
            "The saved Pod0 identity was not restored by NMP. No account state was changed."
        case .wrongRole:
            "The selected identity is not a human account."
        case .expectedPublicKeyMismatch:
            "The restored signer does not match the saved Pod0 identity."
        case .remoteConnectionEndedBeforeReady:
            "The remote signer disconnected before it was ready."
        case .remoteFailure(let message):
            message
        case .rollbackFailed(let original, let rollback):
            "Identity setup failed and NMP could not roll it back safely (\(original); rollback: \(rollback)). No UI identity was adopted."
        case .checkpointRecoveryFailed(let message):
            "NMP removed the live signer but could not clear its checkpoint: \(message) Do not retry or switch accounts."
        case .restoredLocalDetachUnsupported(let issue):
            "Signing out of this restored local account is unavailable until NMP issue #\(issue) supports exact detachment. Nothing was changed."
        case .identitySwitchUnsupported:
            "Switching identities in place is unavailable. Nothing was changed."
        }
    }
}

#if canImport(NMP)
@MainActor
final class Pod0HumanIdentityLifecycle {
    nonisolated static let localSecretReference = "human-local-v1"

    let engineAccess: any Pod0NMPEngineAccess
    let catalogStorage: any Pod0IdentityCatalogStorage
    var localRegistration: NMPAccountRegistration?
    var remoteConnection: NMPNip46Connection?
    var restoredLocalPublicKey: String?

    var state: Pod0HumanIdentityState = .signedOut
    var blocker: Pod0IdentityBlocker?

    init(
        engineAccess: any Pod0NMPEngineAccess,
        catalogStorage: any Pod0IdentityCatalogStorage = KeychainPod0IdentityCatalogStorage()
    ) {
        self.engineAccess = engineAccess
        self.catalogStorage = catalogStorage
    }

    /// NMP restores its own checkpoint during engine construction. Pod0 only
    /// verifies that public capability against its non-secret catalog.
    func restoreHuman() async throws -> Pod0IdentityCatalogEntry? {
        if let blocker {
            switch blocker {
            case .orphanedRestoredLocal, .restoredLocalDetachUnsupported:
                try block(blocker)
            case .identitySwitchUnsupported, .expectedPublicKeyMismatch:
                break
            }
        }
        let active = try engineAccess.engine.activeAccount()
        guard let catalog = try catalogStorage.load(),
              catalog.selectedRole == .human,
              let entry = catalog.entry(for: .human) else {
            guard active == nil else {
                try engineAccess.engine.setActiveAccount(nil)
                restoredLocalPublicKey = active
                try block(.orphanedRestoredLocal(issue: 589))
            }
            restoredLocalPublicKey = nil
            state = .signedOut
            return nil
        }
        try await activateHuman(from: entry, engineActiveAccount: active)
        return entry
    }

    func registerLocal(
        secret: String,
        origin: Pod0IdentityOrigin,
        label: String
    ) async throws -> Pod0IdentityCatalogEntry {
        try requireEmptyIdentitySlot()
        state = .registering
        let registration = try await engineAccess.engine.addAccount(secretKey: secret)
        let entry = Pod0IdentityCatalogEntry(
            role: .human,
            label: label,
            origin: origin,
            expectedPublicKey: registration.publicKey,
            capability: .localKey(secretReference: Self.localSecretReference),
            createdAt: Date()
        )
        var activeMutationAttempted = false
        var catalogWriteAttempted = false
        do {
            activeMutationAttempted = true
            try engineAccess.engine.setActiveAccount(registration.publicKey)
            catalogWriteAttempted = true
            try saveSelected(entry)
        } catch {
            let surfaced = rollbackFailedRegistration(
                registration,
                original: error,
                resetActive: activeMutationAttempted,
                clearCatalog: catalogWriteAttempted
            )
            state = .failed(surfaced.localizedDescription)
            throw surfaced
        }
        localRegistration = registration
        restoredLocalPublicKey = nil
        state = .ready(publicKey: registration.publicKey)
        return entry
    }

    func connectBunker(uri: String, label: String = "Remote signer") async throws -> Pod0IdentityCatalogEntry {
        try requireEmptyIdentitySlot()
        state = .connecting
        blocker = nil
        let connection = try engineAccess.engine.connectNip46(bunkerURI: uri)
        var activeMutationAttempted = false
        var catalogWriteAttempted = false
        do {
            let publicKey = try await awaitReady(connection, expectedPublicKey: nil)
            let entry = Pod0IdentityCatalogEntry(
                role: .human,
                label: label,
                origin: .bunker,
                expectedPublicKey: publicKey,
                capability: .nip46Bunker(uri: uri),
                createdAt: Date()
            )
            activeMutationAttempted = true
            try engineAccess.engine.setActiveAccount(publicKey)
            catalogWriteAttempted = true
            try saveSelected(entry)
            remoteConnection = connection
            restoredLocalPublicKey = nil
            state = .ready(publicKey: publicKey)
            return entry
        } catch {
            connection.close()
            remoteConnection = nil
            let surfaced = rollbackFailedBunkerConnection(
                original: error,
                resetActive: activeMutationAttempted,
                clearCatalog: catalogWriteAttempted
            )
            state = .failed(surfaced.localizedDescription)
            throw surfaced
        }
    }

    private func activateHuman(
        from entry: Pod0IdentityCatalogEntry,
        engineActiveAccount: String?
    ) async throws {
        guard entry.role == .human else { throw Pod0HumanIdentityError.wrongRole }
        blocker = nil
        switch entry.capability {
        case .localKey:
            guard let engineActiveAccount else {
                throw Pod0HumanIdentityError.missingRestoredLocalAccount
            }
            guard engineActiveAccount == entry.expectedPublicKey else {
                try mismatch(expected: entry.expectedPublicKey, actual: engineActiveAccount)
            }
            localRegistration = nil
            restoredLocalPublicKey = engineActiveAccount
            state = .ready(publicKey: engineActiveAccount)
        case .nip46Bunker(let uri):
            guard engineActiveAccount == nil else {
                try engineAccess.engine.setActiveAccount(nil)
                restoredLocalPublicKey = engineActiveAccount
                try block(.orphanedRestoredLocal(issue: 589))
            }
            let connection = try engineAccess.engine.connectNip46(bunkerURI: uri)
            do {
                let publicKey = try await awaitReady(
                    connection,
                    expectedPublicKey: entry.expectedPublicKey
                )
                try engineAccess.engine.setActiveAccount(publicKey)
                remoteConnection = connection
                restoredLocalPublicKey = nil
                state = .ready(publicKey: publicKey)
            } catch {
                connection.close()
                remoteConnection = nil
                throw error
            }
        }
    }

    private func requireEmptyIdentitySlot() throws {
        if blocker != nil {
            try block(.identitySwitchUnsupported)
        }
        let hasCatalog = try catalogStorage.load() != nil
        let hasActiveAccount = try engineAccess.engine.activeAccount() != nil
        if hasCatalog || hasActiveAccount {
            try block(.identitySwitchUnsupported)
        }
    }

    func saveSelected(_ entry: Pod0IdentityCatalogEntry) throws {
        var catalog = Pod0IdentityCatalog(selectedRole: .human)
        catalog.upsert(entry)
        try catalogStorage.save(catalog)
    }

}
#endif
