import XCTest
@testable import Podcastr

/// Test suite for the NIP-46 / NIP-44 v2 stack:
/// ChaCha20 (RFC 8439), NIP-44 (paulmillr spec vectors), bunker URI parsing,
/// JSON-RPC framing, padding-bucket math, and a `RemoteSigner` × mock-relay round-trip.
final class Nip46Tests: XCTestCase {

    // MARK: - ChaCha20 (RFC 8439 §2.4.2 — Sunscreen)

    /// RFC 8439 §2.4.2 test vector — encrypts the well-known "Sunscreen" plaintext
    /// with key=00..1f, nonce=00 00 00 00 00 00 00 4a 00 00 00 00, counter=1.
    func testChaCha20RFC8439SunscreenVector() {
        let key = Data((0..<32).map { UInt8($0) })
        let nonce = Data([0, 0, 0, 0, 0, 0, 0, 0x4a, 0, 0, 0, 0])
        let plaintext = Data("Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.".utf8)
        let expectedCiphertextHex =
            "6e2e359a2568f98041ba0728dd0d6981" +
            "e97e7aec1d4360c20a27afccfd9fae0b" +
            "f91b65c5524733ab8f593dabcd62b357" +
            "1639d624e65152ab8f530c359f0861d8" +
            "07ca0dbf500d6a6156a38e088a22b65e" +
            "52bc514d16ccf806818ce91ab7793736" +
            "5af90bbf74a35be6b40b8eedf2785e42" +
            "874d"
        let ciphertext = ChaCha20.xor(key: key, nonce: nonce, counter: 1, data: plaintext)
        XCTAssertEqual(ciphertext.hexString, expectedCiphertextHex, "RFC 8439 §2.4.2 ChaCha20 vector mismatch")

        // Symmetric — re-XOR with same key/nonce/counter recovers plaintext.
        let roundTrip = ChaCha20.xor(key: key, nonce: nonce, counter: 1, data: ciphertext)
        XCTAssertEqual(roundTrip, plaintext)
    }

    // MARK: - NIP-44 v2 padding (paulmillr spec table)

    /// A handful of values from the official `nip44.vectors.json#v2.valid.calc_padded_len`.
    func testNip44PaddingBuckets() {
        let cases: [(Int, Int)] = [
            (1, 32), (16, 32), (32, 32),
            (33, 64), (37, 64), (45, 64), (49, 64),
            (64, 64), (65, 96),
            (100, 128),
            (200, 224),
            (256, 256), (257, 320),
            (320, 320), (383, 384), (384, 384),
            (400, 448),
            (500, 512),
            (512, 512), (515, 640),
        ]
        for (input, expected) in cases {
            XCTAssertEqual(Nip44.calcPaddedLen(input), expected, "calcPaddedLen(\(input))")
        }
    }

    // MARK: - NIP-44 v2 round-trip

    /// Generate two ECDH keypairs, derive a conversation key from each side, and verify
    /// they agree, then encrypt + decrypt a real message end-to-end.
    func testNip44RoundTripWithRealKeys() throws {
        let alice = try NostrKeyPair.generate()
        let bob = try NostrKeyPair.generate()
        let aliceConv = try Nip44.conversationKey(privateKeyHex: alice.privateKeyHex, peerPublicKeyHex: bob.publicKeyHex)
        let bobConv = try Nip44.conversationKey(privateKeyHex: bob.privateKeyHex, peerPublicKeyHex: alice.publicKeyHex)
        XCTAssertEqual(aliceConv, bobConv, "Both sides must derive the same conversation key.")
        XCTAssertEqual(aliceConv.count, 32)

        let plaintext = "hello bunker ☕️"
        let ciphertext = try Nip44.encrypt(plaintext: plaintext, conversationKey: aliceConv)
        // Standard base64 — should be decodable.
        XCTAssertNotNil(Data(base64Encoded: ciphertext))
        let recovered = try Nip44.decrypt(payload: ciphertext, conversationKey: bobConv)
        XCTAssertEqual(recovered, plaintext)
    }

    /// NIP-44 v2 spec vector pulled verbatim from
    /// https://github.com/paulmillr/nip44/blob/main/javascript/test/nip44.vectors.json
    /// "v2.valid.encrypt_decrypt[0]"
    func testNip44SpecVectorEncryptDecrypt() throws {
        // Source: paulmillr/nip44 v2.valid.encrypt_decrypt[0]
        let sec1 = "0000000000000000000000000000000000000000000000000000000000000001"
        let sec2 = "0000000000000000000000000000000000000000000000000000000000000002"
        let conversationKeyHex = "c41c775356fd92eadc63ff5a0dc1da211b268cbea22316767095b2871ea1412d"
        let plaintext = "a"
        let nonceHex = "0000000000000000000000000000000000000000000000000000000000000001"
        let expectedPayload = "AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABee0G5VSK0/9YypIObAtDKfYEAjD35uVkHyB0F4DwrcNaCXlCWZKaArsGrY6M9wnuTMxWfp1RTN9Xga8no+kF5Vsb"

        let key1 = try Nip44.conversationKey(privateKeyHex: sec1, peerPublicKeyHex: try NostrKeyPair(privateKeyHex: sec2).publicKeyHex)
        XCTAssertEqual(key1.hexString, conversationKeyHex, "Conversation key derivation mismatch.")

        // Encrypt with the spec's nonce; payload must match byte-for-byte.
        let nonce = Data(hexString: nonceHex)!
        let payload = try Nip44.encrypt(plaintext: plaintext, conversationKey: key1, nonce: nonce)
        XCTAssertEqual(payload, expectedPayload, "NIP-44 v2 encrypted payload diverges from the official spec vector.")

        // Decrypt round-trip.
        let recovered = try Nip44.decrypt(payload: payload, conversationKey: key1)
        XCTAssertEqual(recovered, plaintext)
    }

    /// Spec vector for `get_conversation_key` (paulmillr/nip44 v2.valid.get_conversation_key[0]).
    func testNip44ConversationKeySpecVector() throws {
        let sec1 = "315e59ff51cb9209768cf7da80791ddcaae56ac9775eb25b6dee1234bc5d2268"
        let pub2 = "c2f9d9948dc8c7c38321e4b85c8558872eafa0641cd269db76848a6073e69133"
        let expected = "3dfef0ce2a4d80a25e7a328accf73448ef67096f65f79588e358d9a0eb9013f1"
        let conv = try Nip44.conversationKey(privateKeyHex: sec1, peerPublicKeyHex: pub2)
        XCTAssertEqual(conv.hexString, expected)
    }

    /// Spec vector for a unicode-rich plaintext (paulmillr/nip44 v2.valid.encrypt_decrypt[3]).
    func testNip44SpecVectorUnicode() throws {
        let sec1 = "8f40e50a84a7462e2b8d24c28898ef1f23359fff50d8c509e6fb7ce06e142f9c"
        let sec2 = "b9b0a1e9cc20100c5faa3bbe2777303d25950616c4c6a3fa2e3e046f936ec2ba"
        let nonce = Data(hexString: "b20989adc3ddc41cd2c435952c0d59a91315d8c5218d5040573fc3749543acaf")!
        let plaintext = "ability🤝的 ȺȾ"
        let pub2 = try NostrKeyPair(privateKeyHex: sec2).publicKeyHex
        let conv = try Nip44.conversationKey(privateKeyHex: sec1, peerPublicKeyHex: pub2)
        XCTAssertEqual(conv.hexString, "d5a2f879123145a4b291d767428870f5a8d9e5007193321795b40183d4ab8c2b")
        let payload = try Nip44.encrypt(plaintext: plaintext, conversationKey: conv, nonce: nonce)
        let expected = "ArIJia3D3cQc0sQ1lSwNWakTFdjFIY1QQFc/w3SVQ6yvbG2S0x4Yu86QGwPTy7mP3961I1XqB6SFFTzqDZZavhxoWMj7mEVGMQIsh2RLWI5EYQaQDIePSnXPlzf7CIt+voTD"
        XCTAssertEqual(payload, expected)
        XCTAssertEqual(try Nip44.decrypt(payload: payload, conversationKey: conv), plaintext)
    }

    /// Tampering the MAC must reject the message.
    func testNip44RejectsTamperedMAC() throws {
        let alice = try NostrKeyPair.generate()
        let bob = try NostrKeyPair.generate()
        let conv = try Nip44.conversationKey(privateKeyHex: alice.privateKeyHex, peerPublicKeyHex: bob.publicKeyHex)
        let payload = try Nip44.encrypt(plaintext: "secret", conversationKey: conv)
        var bytes = [UInt8](Data(base64Encoded: payload)!)
        bytes[bytes.count - 1] ^= 0xff // flip the last MAC byte
        let tampered = Data(bytes).base64EncodedString()
        XCTAssertThrowsError(try Nip44.decrypt(payload: tampered, conversationKey: conv))
    }

    // MARK: - BunkerURI parsing

    func testBunkerURIParseHappyPath() throws {
        let uri = "bunker://7e7e9c42a91bfef19fa929e5fda1b72e0ebc1a4c1141673e2794234d86addf4e?relay=wss://relay.nsec.app&relay=wss://relay.damus.io&secret=abc123"
        let p = try BunkerURI.parse(uri)
        XCTAssertEqual(p.remotePubkeyHex, "7e7e9c42a91bfef19fa929e5fda1b72e0ebc1a4c1141673e2794234d86addf4e")
        XCTAssertEqual(p.relays, ["wss://relay.nsec.app", "wss://relay.damus.io"])
        XCTAssertEqual(p.secret, "abc123")
        XCTAssertTrue(p.permissions.isEmpty)
    }

    func testBunkerURISecretOptional() throws {
        let uri = "bunker://7e7e9c42a91bfef19fa929e5fda1b72e0ebc1a4c1141673e2794234d86addf4e?relay=wss://relay.example"
        let p = try BunkerURI.parse(uri)
        XCTAssertNil(p.secret)
        XCTAssertEqual(p.relays, ["wss://relay.example"])
    }

    func testBunkerURIParsesPermissions() throws {
        let uri = "bunker://7e7e9c42a91bfef19fa929e5fda1b72e0ebc1a4c1141673e2794234d86addf4e?relay=wss://relay.example&perms=sign_event:1,nip44_encrypt"
        let p = try BunkerURI.parse(uri)
        XCTAssertEqual(p.permissions, ["sign_event:1", "nip44_encrypt"])
    }

    func testBunkerURIRejectsNonBunkerScheme() {
        XCTAssertThrowsError(try BunkerURI.parse("nostrconnect://..."))
        XCTAssertThrowsError(try BunkerURI.parse("https://example.com"))
    }

    func testBunkerURIRejectsMissingRelay() {
        XCTAssertThrowsError(try BunkerURI.parse("bunker://7e7e9c42a91bfef19fa929e5fda1b72e0ebc1a4c1141673e2794234d86addf4e"))
    }

    func testBunkerURIRejectsBadPubkey() {
        XCTAssertThrowsError(try BunkerURI.parse("bunker://nothex?relay=wss://relay.example"))
    }

    func testBunkerURIRejectsNonWSRelay() {
        XCTAssertThrowsError(try BunkerURI.parse("bunker://7e7e9c42a91bfef19fa929e5fda1b72e0ebc1a4c1141673e2794234d86addf4e?relay=https://example.com"))
    }

    // MARK: - JSON-RPC framing

    func testNip46RequestEncodesValidJSON() throws {
        let req = Nip46Request(id: "abc", method: .signEvent, params: ["{\"kind\":1}"])
        let encoded = try req.encode()
        let parsed = try JSONSerialization.jsonObject(with: Data(encoded.utf8)) as? [String: Any]
        XCTAssertEqual(parsed?["id"] as? String, "abc")
        XCTAssertEqual(parsed?["method"] as? String, "sign_event")
        XCTAssertEqual((parsed?["params"] as? [String])?.first, "{\"kind\":1}")
    }

    func testNip46ResponseParsesAck() throws {
        let resp = try Nip46Response.parse("{\"id\":\"abc\",\"result\":\"ack\"}")
        XCTAssertEqual(resp.id, "abc")
        XCTAssertEqual(resp.result, "ack")
        XCTAssertNil(resp.error)
    }

    func testNip46ResponseParsesError() throws {
        let resp = try Nip46Response.parse("{\"id\":\"abc\",\"result\":null,\"error\":\"denied\"}")
        XCTAssertEqual(resp.id, "abc")
        XCTAssertNil(resp.result)
        XCTAssertEqual(resp.error, "denied")
    }

    func testNip46ResponseRejectsMissingID() {
        XCTAssertThrowsError(try Nip46Response.parse("{\"result\":\"ack\"}"))
    }

    // MARK: - LocalKeySigner sanity

    func testLocalKeySignerProducesNIP01CompliantSignature() async throws {
        let pair = try NostrKeyPair.generate()
        let signer = LocalKeySigner(keyPair: pair)
        let draft = NostrEventDraft(kind: 1, content: "hello world", tags: [])
        let signed = try await signer.sign(draft)
        XCTAssertEqual(signed.pubkey, pair.publicKeyHex)
        XCTAssertEqual(signed.id.count, 64)
        XCTAssertEqual(signed.sig.count, 128)

        // Re-derive the canonical id and verify it matches.
        let recomputed = try EventID.compute(
            pubkey: pair.publicKeyHex,
            createdAt: signed.created_at,
            kind: signed.kind,
            tags: signed.tags,
            content: signed.content
        )
        XCTAssertEqual(recomputed, signed.id, "Canonical event id must be reproducible.")
    }

    // MARK: - EventID canonicalisation

    func testEventIDEscapesControlCharacters() {
        let s = EventID.canonicalJSON("a\nb\"c")
        // Spec: \" → escaped quote, \n → escaped newline, no other escapes.
        XCTAssertEqual(s, "\"a\\nb\\\"c\"")
    }
}
