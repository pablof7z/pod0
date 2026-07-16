import XCTest
@testable import Podcastr

final class AgentNostrSignerTests: XCTestCase {
    func testNeutralEventModelCodableRoundTrip() throws {
        let event = SignedNostrEvent(
            id: String(repeating: "a", count: 64),
            pubkey: String(repeating: "b", count: 64),
            created_at: 1_700_000_000,
            kind: 1,
            tags: [["t", "agent"]],
            content: "hello",
            sig: String(repeating: "c", count: 128)
        )

        let decoded = try JSONDecoder().decode(
            SignedNostrEvent.self,
            from: JSONEncoder().encode(event)
        )
        XCTAssertEqual(decoded, event)
    }

    func testAgentLocalSignerProducesCanonicalEventID() async throws {
        let keyPair = try NostrKeyPair.generate()
        let signer = LocalKeySigner(keyPair: keyPair)
        let draft = NostrEventDraft(
            kind: 1,
            content: "agent-authored",
            tags: [["t", "agent"]],
            createdAt: 1_700_000_000
        )

        let signed = try await signer.sign(draft)

        XCTAssertEqual(signed.pubkey, keyPair.publicKeyHex)
        XCTAssertEqual(
            signed.id,
            try EventID.compute(
                pubkey: keyPair.publicKeyHex,
                createdAt: draft.createdAt,
                kind: draft.kind,
                tags: draft.tags,
                content: draft.content
            )
        )
        XCTAssertEqual(signed.sig.count, 128)
    }
}
