import Foundation

#if canImport(NMP)
import NMP
#endif

enum Pod0IdentityBlocker: Sendable, Codable, Equatable {
    /// Upstream pablof7z/nmp#571: a newly paired client-initiated connection
    /// cannot yet be checkpointed and reopened after a cold launch.
    case clientInitiatedNip46CheckpointUnsupported(issue: Int)
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

enum Pod0HumanIdentityError: Error, Equatable {
    case missingHumanEntry
    case missingLocalSecret
    case wrongRole
    case expectedPublicKeyMismatch(expected: String, actual: String)
    case remoteConnectionEndedBeforeReady
    case remoteFailure(String)
}

#if canImport(NMP)
/// Owns the one live human capability attached to Pod0's shared NMP engine.
/// Account inventory remains app policy, while all signing stays in NMP.
@MainActor
final class Pod0HumanIdentityLifecycle {
    nonisolated static let localSecretReference = "human-local-v1"

    private let engineAccess: any Pod0NMPEngineAccess
    private let secretStore: any NMPLocalAccountCheckpoint
    private let catalogStorage: any Pod0IdentityCatalogStorage
    private var localRegistration: NMPAccountRegistration?
    private var remoteConnection: NMPNip46Connection?

    private(set) var state: Pod0HumanIdentityState = .signedOut
    private(set) var blocker: Pod0IdentityBlocker?

    init(
        engineAccess: any Pod0NMPEngineAccess,
        secretStore: any NMPLocalAccountCheckpoint,
        catalogStorage: any Pod0IdentityCatalogStorage = KeychainPod0IdentityCatalogStorage()
    ) {
        self.engineAccess = engineAccess
        self.secretStore = secretStore
        self.catalogStorage = catalogStorage
    }

    /// Restores only a clean-start catalog entry. A local secret is loaded
    /// once, registered with NMP, and retained by its opaque registration so
    /// sign-out can detach that exact capability even after a cold launch.
    func restoreHuman() async throws -> Pod0IdentityCatalogEntry? {
        guard let catalog = try catalogStorage.load(),
              catalog.selectedRole == .human,
              let entry = catalog.entry(for: .human) else {
            try engineAccess.engine.setActiveAccount(nil)
            state = .signedOut
            return nil
        }
        try await activateHuman(from: entry)
        return entry
    }

    func registerLocal(
        secret: String,
        origin: Pod0IdentityOrigin,
        label: String
    ) async throws -> Pod0IdentityCatalogEntry {
        state = .registering
        let reference = Self.localSecretReference
        let previousSecret = try secretStore.loadSecretKey()
        let previousCatalog = try catalogStorage.load()
        let previousActive = try engineAccess.engine.activeAccount()
        let previousRegistration = localRegistration
        let previousRemote = remoteConnection
        var registration: NMPAccountRegistration?
        do {
            let added = try await engineAccess.engine.addAccount(secretKey: secret)
            registration = added
            let entry = Pod0IdentityCatalogEntry(
                role: .human,
                label: label,
                origin: origin,
                expectedPublicKey: added.publicKey,
                capability: .localKey(secretReference: reference),
                createdAt: Date()
            )
            try secretStore.saveSecretKey(secret)
            try saveSelected(entry)
            try engineAccess.engine.setActiveAccount(added.publicKey)
            if let previousRegistration {
                _ = try engineAccess.engine.removeAccount(previousRegistration)
            }
            previousRemote?.close()
            localRegistration = added
            remoteConnection = nil
            state = .ready(publicKey: added.publicKey)
            return entry
        } catch {
            if let registration {
                _ = try? engineAccess.engine.removeAccount(registration)
            }
            if let previousSecret {
                try? secretStore.saveSecretKey(previousSecret)
            } else {
                try? secretStore.clear()
            }
            if let previousCatalog {
                try? catalogStorage.save(previousCatalog)
            } else {
                try? catalogStorage.clear()
            }
            try? engineAccess.engine.setActiveAccount(previousActive)
            state = .failed(String(describing: error))
            throw error
        }
    }

    func connectBunker(uri: String, label: String = "Remote signer") async throws -> Pod0IdentityCatalogEntry {
        state = .connecting
        blocker = nil
        let connection = try engineAccess.engine.connectNip46(bunkerURI: uri)
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
            try retireLocalCapability()
            try saveSelected(entry)
            try engineAccess.engine.setActiveAccount(publicKey)
            remoteConnection?.close()
            remoteConnection = connection
            state = .ready(publicKey: publicKey)
            return entry
        } catch {
            connection.close()
            state = .failed(String(describing: error))
            throw error
        }
    }

    /// NMP #571 does not expose a secure invitation checkpoint/restore API.
    /// Refuse the flow instead of creating an app-owned second transport.
    func connectClientInitiated(relays _: [String]) throws -> Never {
        try block(.clientInitiatedNip46CheckpointUnsupported(issue: 571))
    }

    func cachePreservingSignOut() throws {
        try engineAccess.engine.setActiveAccount(nil)
        remoteConnection?.close()
        remoteConnection = nil
        try retireLocalCapability()
        try catalogStorage.clear()
        blocker = nil
        state = .signedOut
    }

    private func activateHuman(from entry: Pod0IdentityCatalogEntry) async throws {
        guard entry.role == .human else { throw Pod0HumanIdentityError.wrongRole }
        blocker = nil
        switch entry.capability {
        case .localKey(let reference):
            try await registerStoredLocal(entry: entry, reference: reference)
        case .nip46Bunker(let uri):
            let connection = try engineAccess.engine.connectNip46(bunkerURI: uri)
            remoteConnection = connection
            let publicKey = try await awaitReady(connection, expectedPublicKey: entry.expectedPublicKey)
            try engineAccess.engine.setActiveAccount(publicKey)
            state = .ready(publicKey: publicKey)
        case .nip46ClientInitiated:
            try block(.clientInitiatedNip46CheckpointUnsupported(issue: 571))
        case .reservedForLaterMilestone:
            throw Pod0HumanIdentityError.wrongRole
        }
    }

    private func registerStoredLocal(
        entry: Pod0IdentityCatalogEntry,
        reference: String
    ) async throws {
        state = .registering
        guard let secret = try secretStore.loadSecretKey(),
              !secret.isEmpty else {
            state = .failed("Local identity secret is unavailable.")
            throw Pod0HumanIdentityError.missingLocalSecret
        }
        let registration = try await engineAccess.engine.addAccount(secretKey: secret)
        guard registration.publicKey == entry.expectedPublicKey else {
            _ = try? engineAccess.engine.removeAccount(registration)
            try mismatch(expected: entry.expectedPublicKey, actual: registration.publicKey)
        }
        localRegistration = registration
        try engineAccess.engine.setActiveAccount(registration.publicKey)
        state = .ready(publicKey: registration.publicKey)
    }

    private func awaitReady(
        _ connection: NMPNip46Connection,
        expectedPublicKey: String?
    ) async throws -> String {
        for await connectionState in connection.states {
            switch connectionState {
            case .authorizationRequired(let url):
                state = .authorizationRequired(url)
            case .ready(let publicKey):
                if let expectedPublicKey, publicKey != expectedPublicKey {
                    try mismatch(expected: expectedPublicKey, actual: publicKey)
                }
                return publicKey
            case .failed(let failure):
                throw Pod0HumanIdentityError.remoteFailure(String(describing: failure))
            case .connecting, .available, .unavailable, .relayAuthentication, .connected:
                continue
            }
        }
        throw Pod0HumanIdentityError.remoteConnectionEndedBeforeReady
    }

    private func saveSelected(_ entry: Pod0IdentityCatalogEntry) throws {
        var catalog = Pod0IdentityCatalog(selectedRole: .human)
        catalog.upsert(entry)
        try catalogStorage.save(catalog)
    }

    private func retireLocalCapability() throws {
        let checkpoint = try secretStore.loadSecretKey()
        try secretStore.clear()
        if let localRegistration {
            do {
                _ = try engineAccess.engine.removeAccount(localRegistration)
            } catch {
                if let checkpoint {
                    try? secretStore.saveSecretKey(checkpoint)
                }
                throw error
            }
            self.localRegistration = nil
        }
    }

    private func mismatch(expected: String, actual: String) throws -> Never {
        let mismatch = Pod0IdentityBlocker.expectedPublicKeyMismatch(expected: expected, actual: actual)
        blocker = mismatch
        state = .blocked(mismatch)
        throw Pod0HumanIdentityError.expectedPublicKeyMismatch(expected: expected, actual: actual)
    }

    private func block(_ reason: Pod0IdentityBlocker) throws -> Never {
        blocker = reason
        state = .blocked(reason)
        throw Pod0HumanIdentityError.remoteFailure(String(describing: reason))
    }
}
#endif
