import Foundation
import P256K
import Pod0Core

protocol CoreNostrSignerHosting: Sendable {
    func execute(_ request: HostRequest) async -> HostObservation
}

struct CoreNostrSignerCredential: Codable, Sendable, Equatable {
    let accountID: UUID
    let privateKeyHex: String
    let publicKeyHex: String
}

protocol CoreNostrSignerCredentialStoring: Sendable {
    func load() async throws -> CoreNostrSignerCredential?
    func save(_ credential: CoreNostrSignerCredential) async throws
    func delete() async throws
}

actor KeychainCoreNostrSignerCredentialStore: CoreNostrSignerCredentialStoring {
    private let service: String
    private let account: String

    init(
        service: String = "com.pod0.nostr.signer",
        account: String = "primary-local-key"
    ) {
        self.service = service
        self.account = account
    }

    func load() throws -> CoreNostrSignerCredential? {
        guard let value = try KeychainStore.readString(service: service, account: account),
              let data = value.data(using: .utf8)
        else { return nil }
        return try JSONDecoder().decode(CoreNostrSignerCredential.self, from: data)
    }

    func save(_ credential: CoreNostrSignerCredential) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(credential)
        guard let value = String(data: data, encoding: .utf8) else {
            throw CoreNostrSignerHostError.invalidCredential
        }
        try KeychainStore.saveString(value, service: service, account: account)
    }

    func delete() throws {
        try KeychainStore.deleteString(service: service, account: account)
    }
}

/// Executes the platform secure-storage and cryptographic primitives only.
/// Rust owns signer lifecycle, identity policy, retries, and exact-event
/// verification.
actor CoreNostrSignerHost: CoreNostrSignerHosting {
    private let store: any CoreNostrSignerCredentialStoring

    init(
        store: any CoreNostrSignerCredentialStoring =
            KeychainCoreNostrSignerCredentialStore()
    ) {
        self.store = store
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        do {
            try Task.checkCancellation()
            return switch request {
            case .provisionNostrSignerCredential:
                try await provision()
            case .restoreNostrSignerCredential(let accountID, let expectedAuthorHex):
                try await restore(accountID: accountID, expectedAuthorHex: expectedAuthorHex)
            case .signNostrEvent(let request):
                try await sign(request)
            case .deleteNostrSignerCredential(let accountID):
                try await delete(accountID: accountID)
            default:
                .failed(code: .invalidResponse, safeDetail: "Invalid signer capability request")
            }
        } catch is CancellationError {
            return .cancelled
        } catch let error as CoreNostrSignerHostError {
            return error.observation
        } catch {
            return .failed(
                code: .platformFailure,
                safeDetail: "Secure signer capability failed"
            )
        }
    }

    private func provision() async throws -> HostObservation {
        if let credential = try await store.load() {
            try validate(credential)
            return ready(credential)
        }
        let keyPair = try NostrKeyPair.generate()
        let credential = CoreNostrSignerCredential(
            accountID: UUID(),
            privateKeyHex: keyPair.privateKeyHex,
            publicKeyHex: keyPair.publicKeyHex
        )
        try await store.save(credential)
        return ready(credential)
    }

    private func restore(
        accountID: SignerAccountId,
        expectedAuthorHex: String
    ) async throws -> HostObservation {
        guard let credential = try await store.load() else {
            throw CoreNostrSignerHostError.credentialUnavailable
        }
        try validate(credential)
        guard accountID.uuid == credential.accountID,
              expectedAuthorHex == credential.publicKeyHex
        else {
            throw CoreNostrSignerHostError.identityMismatch
        }
        return ready(credential)
    }

    private func sign(_ request: NostrSigningRequest) async throws -> HostObservation {
        guard let credential = try await store.load() else {
            throw CoreNostrSignerHostError.credentialUnavailable
        }
        try validate(credential)
        guard request.accountId.uuid == credential.accountID,
              request.expectedAuthorHex == credential.publicKeyHex,
              let message = Data(hexString: request.eventIdHex),
              message.count == 32
        else {
            throw CoreNostrSignerHostError.identityMismatch
        }
        let keyData = Data(hexString: credential.privateKeyHex)
        guard let keyData, keyData.count == 32 else {
            throw CoreNostrSignerHostError.invalidCredential
        }
        let key = try P256K.Schnorr.PrivateKey(dataRepresentation: keyData)
        var messageBytes = [UInt8](message)
        var auxiliaryRandomness = [UInt8](repeating: 0, count: 32)
        var generator = SystemRandomNumberGenerator()
        for index in auxiliaryRandomness.indices {
            auxiliaryRandomness[index] = UInt8.random(in: .min ... .max, using: &generator)
        }
        let signature = try key.signature(
            message: &messageBytes,
            auxiliaryRand: &auxiliaryRandomness,
            strict: true
        )
        return .nostrEventSigned(value: NostrSignatureObservation(
            accountId: request.accountId,
            eventIdHex: request.eventIdHex,
            signatureHex: signature.dataRepresentation.hexString
        ))
    }

    private func delete(accountID: SignerAccountId) async throws -> HostObservation {
        if let credential = try await store.load(),
           accountID.uuid != credential.accountID {
            throw CoreNostrSignerHostError.identityMismatch
        }
        try await store.delete()
        return .nostrSignerCredentialDeleted(accountId: accountID)
    }

    private func ready(_ credential: CoreNostrSignerCredential) -> HostObservation {
        .nostrSignerCredentialReady(
            accountId: SignerAccountId(uuid: credential.accountID),
            publicKeyHex: credential.publicKeyHex
        )
    }

    private func validate(_ credential: CoreNostrSignerCredential) throws {
        guard let privateKey = Data(hexString: credential.privateKeyHex),
              privateKey.count == 32
        else {
            throw CoreNostrSignerHostError.invalidCredential
        }
        let key = try P256K.Schnorr.PrivateKey(dataRepresentation: privateKey)
        guard Data(key.xonly.bytes).hexString == credential.publicKeyHex else {
            throw CoreNostrSignerHostError.invalidCredential
        }
    }
}

private enum CoreNostrSignerHostError: Error {
    case credentialUnavailable
    case identityMismatch
    case invalidCredential

    var observation: HostObservation {
        switch self {
        case .credentialUnavailable:
            .failed(code: .providerUnavailable, safeDetail: "Secure signer credential unavailable")
        case .identityMismatch:
            .failed(code: .invalidResponse, safeDetail: "Secure signer identity mismatch")
        case .invalidCredential:
            .failed(code: .invalidResponse, safeDetail: "Secure signer credential is invalid")
        }
    }
}
