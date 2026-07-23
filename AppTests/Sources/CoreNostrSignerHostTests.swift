import Foundation
import P256K
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreNostrSignerHostTests: XCTestCase {
    func testProvisionIsIdempotentAndNeverExportsPrivateMaterial() async throws {
        let store = InMemoryNostrSignerCredentialStore()
        let host = CoreNostrSignerHost(store: store)

        let first = await host.execute(.provisionNostrSignerCredential)
        let second = await host.execute(.provisionNostrSignerCredential)

        guard case .nostrSignerCredentialReady(let firstID, let firstAuthor) = first,
              case .nostrSignerCredentialReady(let secondID, let secondAuthor) = second
        else {
            return XCTFail("Expected signer-ready observations")
        }
        XCTAssertEqual(firstID, secondID)
        XCTAssertEqual(firstAuthor, secondAuthor)
        XCTAssertEqual(firstAuthor.count, 64)
        let saveCount = await store.saveCount()
        XCTAssertEqual(saveCount, 1)
    }

    func testRestoreRequiresExactDurableIdentity() async {
        let store = InMemoryNostrSignerCredentialStore()
        let host = CoreNostrSignerHost(store: store)
        let provisioned = await host.execute(.provisionNostrSignerCredential)
        guard case .nostrSignerCredentialReady(let accountID, let author) = provisioned else {
            return XCTFail("Expected signer-ready observation")
        }

        let restored = await host.execute(.restoreNostrSignerCredential(
            accountId: accountID,
            expectedAuthorHex: author
        ))
        XCTAssertEqual(restored, provisioned)

        let mismatched = await host.execute(.restoreNostrSignerCredential(
            accountId: SignerAccountId(uuid: UUID()),
            expectedAuthorHex: author
        ))
        guard case .failed(let code, _) = mismatched else {
            return XCTFail("Expected identity mismatch")
        }
        XCTAssertEqual(code, .invalidResponse)
    }

    func testSignProducesAValidSignatureForTheExactEventID() async throws {
        let store = InMemoryNostrSignerCredentialStore()
        let host = CoreNostrSignerHost(store: store)
        let provisioned = await host.execute(.provisionNostrSignerCredential)
        guard case .nostrSignerCredentialReady(let accountID, let author) = provisioned else {
            return XCTFail("Expected signer-ready observation")
        }
        let eventID = String(repeating: "ab", count: 32)
        let request = NostrSigningRequest(
            accountId: accountID,
            eventIdHex: eventID,
            expectedAuthorHex: author,
            createdAtSeconds: 1_700_000_000,
            kind: 1,
            tags: [["t", "pod0"]],
            content: "bounded event"
        )

        let observed = await host.execute(.signNostrEvent(request: request))

        guard case .nostrEventSigned(let value) = observed else {
            return XCTFail("Expected signed-event observation")
        }
        XCTAssertEqual(value.accountId, accountID)
        XCTAssertEqual(value.eventIdHex, eventID)
        let publicKey = P256K.Schnorr.XonlyKey(
            dataRepresentation: try XCTUnwrap(Data(hexString: author))
        )
        let signature = try P256K.Schnorr.SchnorrSignature(
            dataRepresentation: XCTUnwrap(Data(hexString: value.signatureHex))
        )
        var message = [UInt8](try XCTUnwrap(Data(hexString: eventID)))
        XCTAssertTrue(publicKey.isValid(signature, for: &message))
    }

    func testDeleteCannotRemoveAReplacementCredential() async throws {
        let store = InMemoryNostrSignerCredentialStore()
        let host = CoreNostrSignerHost(store: store)
        let provisioned = await host.execute(.provisionNostrSignerCredential)
        guard case .nostrSignerCredentialReady(let oldAccountID, _) = provisioned else {
            return XCTFail("Expected signer-ready observation")
        }
        let replacementUUID = UUID()
        let replacementID = SignerAccountId(uuid: replacementUUID)
        try await store.replaceAccountID(replacementUUID)

        let staleDelete = await host.execute(.deleteNostrSignerCredential(
            accountId: oldAccountID
        ))
        guard case .failed(let code, _) = staleDelete else {
            return XCTFail("Expected stale delete to fail")
        }
        XCTAssertEqual(code, .invalidResponse)
        let retained = try await store.load()
        XCTAssertNotNil(retained)

        let deleted = await host.execute(.deleteNostrSignerCredential(
            accountId: replacementID
        ))
        XCTAssertEqual(
            deleted,
            .nostrSignerCredentialDeleted(accountId: replacementID)
        )
        let removed = try await store.load()
        XCTAssertNil(removed)
    }
}

private actor InMemoryNostrSignerCredentialStore: CoreNostrSignerCredentialStoring {
    private var credential: CoreNostrSignerCredential?
    private var saves = 0

    func load() async throws -> CoreNostrSignerCredential? {
        credential
    }

    func save(_ credential: CoreNostrSignerCredential) async throws {
        self.credential = credential
        saves += 1
    }

    func delete() async throws {
        credential = nil
    }

    func saveCount() -> Int {
        saves
    }

    func replaceAccountID(_ accountID: UUID) throws {
        guard let credential else {
            throw InMemoryNostrSignerCredentialStoreError.missingCredential
        }
        self.credential = CoreNostrSignerCredential(
            accountID: accountID,
            privateKeyHex: credential.privateKeyHex,
            publicKeyHex: credential.publicKeyHex
        )
    }
}

private enum InMemoryNostrSignerCredentialStoreError: Error {
    case missingCredential
}
