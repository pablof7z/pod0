import Foundation

#if canImport(NMP)
import NMP
#endif

enum Pod0IdentityBlocker: Sendable, Codable, Equatable {
    /// Upstream pablof7z/nmp#571: Swift cannot import/checkpoint the legacy
    /// client session private key, so a client-initiated connection cannot be
    /// proven to survive a cold launch without re-pairing.
    case clientInitiatedNip46RestoreUnsupported(issue: Int)
    /// The legacy schema omitted origin. A secret proves bunker-origin; its
    /// absence is ambiguous and therefore cannot be promoted to authority.
    case legacyRemoteOriginAmbiguous
    case expectedPublicKeyMismatch(expected: String, actual: String)
    case legacyRemoteTransportStillRunning
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

protocol Pod0LegacyHumanSecretReading: Sendable {
    /// The secret is returned to the launch coordinator once, passed directly
    /// to NMP registration, and never copied into the migration record/logs.
    func readLocalHumanSecret(reference: String) throws -> String?
}

struct KeychainLegacyHumanSecretReader: Pod0LegacyHumanSecretReading {
    private let service: String

    init(bundleIdentifier: String = Bundle.main.bundleIdentifier ?? "Podcastr") {
        service = "\(bundleIdentifier).user-identity"
    }

    func readLocalHumanSecret(reference: String) throws -> String? {
        guard reference == "legacy-human-private-key" else { return nil }
        return try KeychainStore.readString(service: service, account: "user-private-key-hex")
    }
}

#if canImport(NMP)
/// Owns only the live human capability registrations attached to the shared
/// composition. Account inventory/selection remains in Pod0's catalog.
@MainActor
final class Pod0HumanIdentityLifecycle {
    private let engineAccess: any Pod0NMPEngineAccess
    private let secretReader: any Pod0LegacyHumanSecretReading
    private var localRegistration: NMPAccountRegistration?
    private var remoteConnection: NMPNip46Connection?

    private(set) var state: Pod0HumanIdentityState = .signedOut
    private(set) var blocker: Pod0IdentityBlocker?

    init(
        engineAccess: any Pod0NMPEngineAccess,
        secretReader: any Pod0LegacyHumanSecretReading = KeychainLegacyHumanSecretReader()
    ) {
        self.engineAccess = engineAccess
        self.secretReader = secretReader
    }

    func restoreHuman(
        from entry: Pod0IdentityCatalogEntry,
        legacyRemoteTransportIsStopped: Bool
    ) async throws {
        guard entry.role == .human else { throw Pod0HumanIdentityError.wrongRole }
        blocker = nil
        switch entry.capability {
        case .localKey(let reference):
            try await registerLocal(entry: entry, secretReference: reference)
        case .nip46Bunker(let uri):
            guard legacyRemoteTransportIsStopped else {
                try block(.legacyRemoteTransportStillRunning)
            }
            try await connectBunker(entry: entry, uri: uri)
        case .nip46ClientInitiated:
            try block(.clientInitiatedNip46RestoreUnsupported(issue: 571))
        case .reservedForLaterMilestone:
            throw Pod0HumanIdentityError.wrongRole
        }
    }

    func cachePreservingSignOut() throws {
        try engineAccess.engine.setActiveAccount(nil)
        remoteConnection?.close()
        remoteConnection = nil
        if let localRegistration {
            _ = try engineAccess.engine.removeAccount(localRegistration)
            self.localRegistration = nil
        }
        blocker = nil
        state = .signedOut
    }

    private func registerLocal(
        entry: Pod0IdentityCatalogEntry,
        secretReference: String
    ) async throws {
        if try engineAccess.engine.activeAccount() == entry.expectedPublicKey,
           localRegistration != nil {
            state = .ready(publicKey: entry.expectedPublicKey)
            return
        }
        state = .registering
        guard let secret = try secretReader.readLocalHumanSecret(reference: secretReference),
              !secret.isEmpty else {
            state = .failed("Local identity secret is unavailable.")
            throw Pod0HumanIdentityError.missingLocalSecret
        }
        let registration = try await engineAccess.engine.addAccount(secretKey: secret)
        guard registration.publicKey == entry.expectedPublicKey else {
            _ = try? engineAccess.engine.removeAccount(registration)
            let mismatch = Pod0IdentityBlocker.expectedPublicKeyMismatch(
                expected: entry.expectedPublicKey,
                actual: registration.publicKey
            )
            blocker = mismatch
            state = .blocked(mismatch)
            throw Pod0HumanIdentityError.expectedPublicKeyMismatch(
                expected: entry.expectedPublicKey,
                actual: registration.publicKey
            )
        }
        localRegistration = registration
        try engineAccess.engine.setActiveAccount(registration.publicKey)
        state = .ready(publicKey: registration.publicKey)
    }

    private func connectBunker(entry: Pod0IdentityCatalogEntry, uri: String) async throws {
        remoteConnection?.close()
        state = .connecting
        let connection = try engineAccess.engine.connectNip46(bunkerURI: uri)
        remoteConnection = connection
        for await connectionState in connection.states {
            switch connectionState {
            case .authorizationRequired(let url):
                state = .authorizationRequired(url)
            case .ready(let publicKey):
                guard publicKey == entry.expectedPublicKey else {
                    connection.close()
                    remoteConnection = nil
                    let mismatch = Pod0IdentityBlocker.expectedPublicKeyMismatch(
                        expected: entry.expectedPublicKey,
                        actual: publicKey
                    )
                    blocker = mismatch
                    state = .blocked(mismatch)
                    throw Pod0HumanIdentityError.expectedPublicKeyMismatch(
                        expected: entry.expectedPublicKey,
                        actual: publicKey
                    )
                }
                try engineAccess.engine.setActiveAccount(publicKey)
                state = .ready(publicKey: publicKey)
                return
            case .failed(let failure):
                remoteConnection = nil
                let message = String(describing: failure)
                state = .failed(message)
                throw Pod0HumanIdentityError.remoteFailure(message)
            case .connecting, .available, .unavailable, .relayAuthentication, .connected:
                // `.connected` is intentionally insufficient. Only `.ready`
                // proves the signer was attached to this engine.
                continue
            }
        }
        remoteConnection = nil
        state = .failed("Remote signer ended before becoming ready.")
        throw Pod0HumanIdentityError.remoteConnectionEndedBeforeReady
    }

    private func block(_ reason: Pod0IdentityBlocker) throws -> Never {
        blocker = reason
        state = .blocked(reason)
        throw Pod0HumanIdentityError.remoteFailure(String(describing: reason))
    }
}
#endif

