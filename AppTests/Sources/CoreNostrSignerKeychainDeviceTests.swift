import Foundation
import P256K
import Pod0Core
import Security
import XCTest
@testable import Podcastr

/// Exercises the real iOS Keychain rather than an injected store.
///
/// `CoreNostrSignerHostTests` proves the host's decision logic against an
/// in-memory double, which is why simulator runs could never qualify #137: the
/// simulator keychain does not enforce data-protection classes, so nothing
/// there can show that a credential is actually stored device-only and
/// unlocked-only. These cases talk to `SecItem*` directly and are meaningful
/// only on physical hardware.
///
/// Every case uses a per-test service name and deletes it in `tearDown`, so the
/// owner's production credential at `com.pod0.nostr.signer` is never read,
/// overwritten, or removed.
@MainActor
final class CoreNostrSignerKeychainDeviceTests: XCTestCase {
    private var service = ""
    private let account = "primary-local-key"

    override func setUp() async throws {
        try await super.setUp()
        service = "com.pod0.nostr.signer.devicetest.\(UUID().uuidString)"
        XCTAssertNotEqual(service, "com.pod0.nostr.signer")
    }

    override func tearDown() async throws {
        try? KeychainStore.deleteString(service: service, account: account)
        try await super.tearDown()
    }

    /// The credential survives a real add/read cycle through `SecItem*`.
    func testRealKeychainRoundTripsTheExactCredential() async throws {
        let store = makeStore()
        let expected = try makeCredential()

        try await store.save(expected)
        let loaded = try await store.load()

        XCTAssertEqual(loaded, expected)
    }

    /// The property a simulator cannot prove. `WhenUnlockedThisDeviceOnly` is
    /// what makes a locked-device read fail instead of silently succeeding, and
    /// what keeps the key off backups and off any restored device.
    func testStoredCredentialIsUnlockedOnlyAndNeverLeavesThisDevice() async throws {
        let store = makeStore()
        try await store.save(try makeCredential())

        let attributes = try copyAttributes()
        let accessible = try XCTUnwrap(attributes[kSecAttrAccessible as String] as? String)

        XCTAssertEqual(accessible, kSecAttrAccessibleWhenUnlockedThisDeviceOnly as String)
        XCTAssertEqual(
            attributes[kSecAttrSynchronizable as String] as? Bool,
            false,
            "A synchronizable signing key would leave the device via iCloud Keychain"
        )
    }

    /// Re-provisioning must replace in place. A second generic-password item
    /// under the same key would make reads order-dependent.
    func testSavingTwiceLeavesExactlyOneItemHoldingTheNewestCredential() async throws {
        let store = makeStore()
        let first = try makeCredential()
        let second = try makeCredential()

        try await store.save(first)
        try await store.save(second)

        XCTAssertEqual(try matchingItemCount(), 1)
        let loaded = try await store.load()
        XCTAssertEqual(loaded, second)
    }

    /// Sign-out runs whether or not a credential is present.
    func testDeleteIsIdempotentAgainstTheRealKeychain() async throws {
        let store = makeStore()
        try await store.save(try makeCredential())

        try await store.delete()
        try await store.delete()

        let loaded = try await store.load()
        XCTAssertNil(loaded)
        XCTAssertEqual(try matchingItemCount(), 0)
    }

    /// Full lifecycle against real secure storage: provision, restore, sign,
    /// verify against the frozen event id, delete.
    func testHostLifecycleProducesAVerifiableSignatureFromRealSecureStorage() async throws {
        let host = CoreNostrSignerHost(store: makeStore())

        let provisioned = await host.execute(.provisionNostrSignerCredential)
        guard case .nostrSignerCredentialReady(let accountID, let authorHex) = provisioned else {
            return XCTFail("Expected a signer-ready observation, got \(provisioned)")
        }
        XCTAssertEqual(authorHex.count, 64)

        let restored = await host.execute(
            .restoreNostrSignerCredential(accountId: accountID, expectedAuthorHex: authorHex)
        )
        XCTAssertEqual(
            restored,
            .nostrSignerCredentialReady(accountId: accountID, publicKeyHex: authorHex)
        )

        let eventIDHex = Data((0 ..< 32).map { UInt8($0) }).hexString
        let signed = await host.execute(.signNostrEvent(request: NostrSigningRequest(
            accountId: accountID,
            eventIdHex: eventIDHex,
            expectedAuthorHex: authorHex,
            createdAtSeconds: 1_700_000_000,
            kind: 1,
            tags: [["t", "pod0"]],
            content: "device keychain qualification"
        )))
        guard case .nostrEventSigned(let signature) = signed else {
            return XCTFail("Expected a signature observation, got \(signed)")
        }
        XCTAssertEqual(signature.accountId, accountID)
        XCTAssertEqual(signature.eventIdHex, eventIDHex)
        XCTAssertTrue(
            try verify(
                signatureHex: signature.signatureHex,
                eventIDHex: eventIDHex,
                authorHex: authorHex
            ),
            "The device-stored key must produce a signature the frozen author verifies"
        )

        let deleted = await host.execute(.deleteNostrSignerCredential(accountId: accountID))
        XCTAssertEqual(deleted, .nostrSignerCredentialDeleted(accountId: accountID))
        XCTAssertEqual(try matchingItemCount(), 0)
    }

    /// The wrong-author guard. A locked device returns `errSecInteractionNotAllowed`
    /// from `SecItemCopyMatching`; provisioning must fail rather than read `nil`
    /// and mint a second identity that would re-author every later write.
    func testUnreadableKeychainFailsClosedInsteadOfMintingANewIdentity() async throws {
        let store = UnavailableNostrSignerCredentialStore(
            status: errSecInteractionNotAllowed
        )
        let host = CoreNostrSignerHost(store: store)

        let observation = await host.execute(.provisionNostrSignerCredential)

        guard case .failed = observation else {
            return XCTFail("A locked keychain must fail closed, got \(observation)")
        }
        let saves = await store.saveCount()
        XCTAssertEqual(saves, 0, "Failing to read must never write a replacement identity")
    }

    /// Failure detail crossing the FFI must stay free of key material.
    func testFailureObservationsCarryNoCredentialMaterial() async throws {
        let store = UnavailableNostrSignerCredentialStore(status: errSecInteractionNotAllowed)
        let host = CoreNostrSignerHost(store: store)

        let observation = await host.execute(.provisionNostrSignerCredential)

        guard case .failed(_, let safeDetail) = observation else {
            return XCTFail("Expected a failed observation, got \(observation)")
        }
        let detail = try XCTUnwrap(safeDetail)
        XCTAssertFalse(detail.contains(String(errSecInteractionNotAllowed)))
        XCTAssertEqual(detail, "Secure signer capability failed")
    }

    // MARK: - Helpers

    private func makeStore() -> KeychainCoreNostrSignerCredentialStore {
        KeychainCoreNostrSignerCredentialStore(service: service, account: account)
    }

    private func makeCredential() throws -> CoreNostrSignerCredential {
        let keyPair = try CoreNostrSignerKeyMaterial.generate()
        return CoreNostrSignerCredential(
            accountID: UUID(),
            privateKeyHex: keyPair.privateKeyHex,
            publicKeyHex: keyPair.publicKeyHex
        )
    }

    private func copyAttributes() throws -> [String: Any] {
        var query = deviceQuery()
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        query[kSecReturnAttributes as String] = true
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        XCTAssertEqual(status, errSecSuccess)
        return try XCTUnwrap(result as? [String: Any])
    }

    private func matchingItemCount() throws -> Int {
        var query = deviceQuery()
        query[kSecMatchLimit as String] = kSecMatchLimitAll
        query[kSecReturnAttributes as String] = true
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return 0 }
        XCTAssertEqual(status, errSecSuccess)
        return (result as? [[String: Any]])?.count ?? 0
    }

    private func deviceQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    private func verify(
        signatureHex: String,
        eventIDHex: String,
        authorHex: String
    ) throws -> Bool {
        let signatureData = try XCTUnwrap(Data(hexString: signatureHex))
        let messageData = try XCTUnwrap(Data(hexString: eventIDHex))
        let authorData = try XCTUnwrap(Data(hexString: authorHex))
        let publicKey = P256K.Schnorr.XonlyKey(dataRepresentation: authorData)
        let signature = try P256K.Schnorr.SchnorrSignature(dataRepresentation: signatureData)
        var messageBytes = [UInt8](messageData)
        return publicKey.isValid(signature, for: &messageBytes)
    }
}

/// Reproduces a keychain that cannot be read — the locked-device case.
private actor UnavailableNostrSignerCredentialStore: CoreNostrSignerCredentialStoring {
    private let status: OSStatus
    private var saves = 0

    init(status: OSStatus) {
        self.status = status
    }

    func load() async throws -> CoreNostrSignerCredential? {
        throw KeychainStoreError.unhandledStatus(status)
    }

    func save(_ credential: CoreNostrSignerCredential) async throws {
        saves += 1
    }

    func delete() async throws {}

    func saveCount() -> Int {
        saves
    }
}
