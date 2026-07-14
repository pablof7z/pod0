import Foundation
import XCTest
@testable import Podcastr

/// Round-trip tests for `RemoteSigner` that exercise its actor logic without ever opening
/// a real WebSocket. We inject a mock transport that records outgoing events and lets the
/// test script inbound NIP-46 frames (encrypted with the same NIP-44 conversation key the
/// signer derives from its session key + the bunker key).
final class Nip46RemoteSignerTests: XCTestCase {

    // MARK: - Baseline: connect → ack still works

    /// Sanity check that the refactor didn't break the simple happy path: bunker replies
    /// to the `connect` request with `result == "ack"`, then to `get_public_key` with the
    /// user's pubkey.
    func testConnectHappyPathReturnsUserPubkey() async throws {
        let env = try TestEnv.make()
        env.script { call, helper in
            switch call.method {
            case "connect": await helper.replyAck(to: call)
            case "get_public_key": await helper.replyResult(to: call, result: env.bunkerPubkey)
            default: break
            }
        }
        let pubkey = try await env.signer.connect()
        XCTAssertEqual(pubkey, env.bunkerPubkey)
    }

    // MARK: - Auth_url challenge

    /// Per NIP-46 "Auth Challenges" (https://github.com/nostr-protocol/nips/blob/master/46.md):
    /// bunker may reply to `connect` with `result: "auth_url"` and the URL in `error`. The
    /// signer must NOT consume the continuation — it must keep the request id alive, fire
    /// the auth-challenge callback, and only resume on the eventual real `ack` (same id).
    func testConnectAuthChallengeThenAckSucceeds() async throws {
        let authURL = URL(string: "https://nsec.app/auth/abc123")!
        let env = try TestEnv.make()

        env.script { call, helper in
            switch call.method {
            case "connect":
                // First reply auth_url, then a brief delay, then real ack — same id.
                await helper.replyAuthURL(to: call, url: authURL)
                try? await Task.sleep(for: .milliseconds(50))
                await helper.replyAck(to: call)
            case "get_public_key":
                await helper.replyResult(to: call, result: env.bunkerPubkey)
            default:
                break
            }
        }

        let challengeBox = AuthChallengeBox()
        let pubkey = try await env.signer.connect { url in
            await challengeBox.set(url)
        }
        XCTAssertEqual(pubkey, env.bunkerPubkey)
        let captured = await challengeBox.value
        XCTAssertEqual(captured, authURL, "auth_url callback must fire with the spec URL from `error`")
    }

    /// auth_url alone (no follow-up ack) MUST eventually time out — verifies we don't
    /// silently accept the URL as success.
    func testConnectAuthChallengeWithoutAckTimesOut() async throws {
        let authURL = URL(string: "https://nsec.app/auth/never")!
        let env = try TestEnv.make(requestTimeout: .milliseconds(300), authChallengeTimeout: .milliseconds(300))

        env.script { call, helper in
            if call.method == "connect" {
                await helper.replyAuthURL(to: call, url: authURL)
            }
        }

        do {
            _ = try await env.signer.connect { _ in }
            XCTFail("Expected connect to time out when ack never arrives")
        } catch let error as NostrSignerError {
            switch error {
            case .timedOut: break
            default: XCTFail("Expected .timedOut, got \(error)")
            }
        }
    }

    // MARK: - Multi-relay failover

    /// `bunker://` URIs may list multiple relays. If the first one fails the connect
    /// handshake, the signer should advance to the next relay. We assert which URL the
    /// active transport is bound to after failover.
    func testMultiRelayFailoverPicksSecondRelay() async throws {
        let badURL = URL(string: "wss://relay-down.example")!
        let goodURL = URL(string: "wss://relay-up.example")!

        let bunkerKey = try NostrKeyPair.generate()
        let session = try NostrKeyPair.generate()
        let bunker = BunkerURI(
            remotePubkeyHex: bunkerKey.publicKeyHex,
            relays: [badURL.absoluteString, goodURL.absoluteString],
            secret: nil,
            permissions: []
        )
        let convKey = try Nip44.conversationKey(privateKeyHex: session.privateKeyHex, peerPublicKeyHex: bunkerKey.publicKeyHex)
        let log = AttemptLog()

        // Build publish handlers up front so they're set synchronously at construction —
        // no Task hop, no race where the first publish might land before the handler.
        let badHandler: @Sendable (SignedNostrEvent) async throws -> Void = { _ in
            await log.note(url: badURL, succeeded: false)
            throw NostrSignerError.notConnected
        }
        let goodHandler: @Sendable (SignedNostrEvent, MockRemoteSignerTransport) async throws -> Void = { event, transport in
            await log.note(url: goodURL, succeeded: true)
            let req = try Nip46Request.decryptFromEvent(event, conversationKey: convKey)
            let body: [String: String]
            if req.method == "connect" {
                body = ["id": req.id, "result": "ack"]
            } else {
                body = ["id": req.id, "result": bunkerKey.publicKeyHex]
            }
            let json = String(data: try JSONSerialization.data(withJSONObject: body), encoding: .utf8)!
            let cipher = try Nip44.encrypt(plaintext: json, conversationKey: convKey)
            await transport.deliverInbound(senderPubkey: bunkerKey.publicKeyHex, encryptedContent: cipher)
        }

        // Box for the good-relay mock so the publish handler can route inbound responses
        // back through it. Set immediately after construction (synchronously) so the very
        // first publish on the good relay sees a non-nil reference.
        let goodMockBox = MockHolder()
        let factory: RemoteSignerTransportFactory = { url, sp, bp, on in
            if url == badURL {
                return MockRemoteSignerTransport(
                    relayURL: url, sessionPubkeyHex: sp, bunkerPubkeyHex: bp, onEvent: on,
                    initialOnPublish: badHandler
                )
            } else {
                let wrapper: @Sendable (SignedNostrEvent) async throws -> Void = { event in
                    guard let m = goodMockBox.value else { return }
                    try await goodHandler(event, m)
                }
                let mock = MockRemoteSignerTransport(
                    relayURL: url, sessionPubkeyHex: sp, bunkerPubkeyHex: bp, onEvent: on,
                    initialOnPublish: wrapper
                )
                goodMockBox.set(mock)
                return mock
            }
        }

        let signer = RemoteSigner(
            bunker: bunker,
            sessionKeyPair: session,
            requestTimeout: .seconds(2),
            transportFactory: factory
        )
        let pubkey = try await signer.connect()
        XCTAssertEqual(pubkey, bunkerKey.publicKeyHex)

        let entries = await log.entries
        XCTAssertEqual(entries.first?.url, badURL, "First relay should be tried first")
        XCTAssertTrue(entries.contains { $0.url == goodURL && $0.succeeded }, "Second relay must succeed")
        let active = await signer.activeRelayURL
        XCTAssertEqual(active, goodURL, "After failover, the active relay should be the second one")
    }

    /// If every relay in the URI fails, connect should throw rather than hang.
    func testAllRelaysFailingThrows() async throws {
        let bunkerKey = try NostrKeyPair.generate()
        let session = try NostrKeyPair.generate()
        let bunker = BunkerURI(
            remotePubkeyHex: bunkerKey.publicKeyHex,
            relays: ["wss://r1.example", "wss://r2.example"],
            secret: nil,
            permissions: []
        )
        let factory: RemoteSignerTransportFactory = { url, sp, bp, on in
            MockRemoteSignerTransport(
                relayURL: url, sessionPubkeyHex: sp, bunkerPubkeyHex: bp, onEvent: on,
                initialOnPublish: { _ in throw NostrSignerError.notConnected }
            )
        }
        let signer = RemoteSigner(
            bunker: bunker,
            sessionKeyPair: session,
            requestTimeout: .milliseconds(200),
            transportFactory: factory
        )
        do {
            _ = try await signer.connect()
            XCTFail("Expected connect to throw when all relays fail")
        } catch {
            // Expected.
        }
    }

    // MARK: - Cached pubkey during reconnect

    /// During the brief reconnect window after app restart, callers of
    /// `signer.publicKey()` should get the cached pubkey passed in at init (from
    /// persisted Keychain meta) rather than `.missingPublicKey`.
    func testCachedPubkeyAvailableBeforeHandshake() async throws {
        let bunkerKey = try NostrKeyPair.generate()
        let session = try NostrKeyPair.generate()
        let bunker = BunkerURI(
            remotePubkeyHex: bunkerKey.publicKeyHex,
            relays: ["wss://relay.example"],
            secret: nil,
            permissions: []
        )
        let cached = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        let signer = RemoteSigner(
            bunker: bunker,
            sessionKeyPair: session,
            cachedUserPublicKeyHex: cached,
            transportFactory: { url, sp, bp, on in
                MockRemoteSignerTransport(relayURL: url, sessionPubkeyHex: sp, bunkerPubkeyHex: bp, onEvent: on)
            }
        )
        let pk = try await signer.publicKey()
        XCTAssertEqual(pk, cached, "publicKey() must return the cached value before any handshake")
    }
}

// MARK: - Test scaffolding

private actor AuthChallengeBox {
    private(set) var value: URL?
    func set(_ url: URL) { value = url }
}

private actor AttemptLog {
    struct Entry: Sendable, Equatable { let url: URL; let succeeded: Bool }
    private(set) var entries: [Entry] = []
    func note(url: URL, succeeded: Bool) { entries.append(Entry(url: url, succeeded: succeeded)) }
}

/// Decrypted call as the bunker would see it.
struct DecodedCall: Sendable {
    let id: String
    let method: String
    let params: [String]
}

/// Helper handed to scripts so they can reply with properly-encrypted NIP-46 frames.
actor ScriptHelper {
    private let convKey: Data
    private let bunkerPubkey: String
    private let transport: MockRemoteSignerTransport

    init(convKey: Data, bunkerPubkey: String, transport: MockRemoteSignerTransport) {
        self.convKey = convKey
        self.bunkerPubkey = bunkerPubkey
        self.transport = transport
    }

    func replyAck(to call: DecodedCall) async {
        await sendBody(["id": call.id, "result": "ack"])
    }
    func replyResult(to call: DecodedCall, result: String) async {
        await sendBody(["id": call.id, "result": result])
    }
    /// Spec: `result == "auth_url"`, `error == <URL>`.
    func replyAuthURL(to call: DecodedCall, url: URL) async {
        await sendBody(["id": call.id, "result": "auth_url", "error": url.absoluteString])
    }

    private func sendBody(_ body: [String: String]) async {
        let json = String(data: try! JSONSerialization.data(withJSONObject: body), encoding: .utf8)!
        let cipher = try! Nip44.encrypt(plaintext: json, conversationKey: convKey)
        await transport.deliverInbound(senderPubkey: bunkerPubkey, encryptedContent: cipher)
    }
}

/// Bundles keys, signer, the mock transport, and a script DSL for one test.
final class TestEnv: @unchecked Sendable {
    let bunkerPubkey: String
    let signer: RemoteSigner
    let convKey: Data
    let holder: MockHolder

    private init(bunkerPubkey: String, signer: RemoteSigner, convKey: Data, holder: MockHolder) {
        self.bunkerPubkey = bunkerPubkey
        self.signer = signer
        self.convKey = convKey
        self.holder = holder
    }

    static func make(
        requestTimeout: Duration = .seconds(5),
        authChallengeTimeout: Duration = .seconds(180)
    ) throws -> TestEnv {
        let bunkerKey = try NostrKeyPair.generate()
        let session = try NostrKeyPair.generate()
        let bunker = BunkerURI(
            remotePubkeyHex: bunkerKey.publicKeyHex,
            relays: ["wss://relay.example"],
            secret: nil,
            permissions: []
        )
        let convKey = try Nip44.conversationKey(
            privateKeyHex: session.privateKeyHex,
            peerPublicKeyHex: bunkerKey.publicKeyHex
        )
        let holder = MockHolder()
        let factory: RemoteSignerTransportFactory = { url, sp, bp, on in
            let m = MockRemoteSignerTransport(relayURL: url, sessionPubkeyHex: sp, bunkerPubkeyHex: bp, onEvent: on)
            holder.set(m)
            return m
        }
        let signer = RemoteSigner(
            bunker: bunker,
            sessionKeyPair: session,
            requestTimeout: requestTimeout,
            authChallengeTimeout: authChallengeTimeout,
            transportFactory: factory
        )
        return TestEnv(bunkerPubkey: bunkerKey.publicKeyHex, signer: signer, convKey: convKey, holder: holder)
    }

    /// Install a script run for every publish. The mock is constructed by the signer's
    /// factory the moment `connect()` runs; we install the publish handler proactively
    /// via the holder's onSet hook so the very first publish is already covered.
    func script(_ handler: @escaping @Sendable (DecodedCall, ScriptHelper) async -> Void) {
        let convKey = self.convKey
        let bunkerPub = self.bunkerPubkey
        holder.onSet = { mock in
            Task {
                let helper = ScriptHelper(convKey: convKey, bunkerPubkey: bunkerPub, transport: mock)
                await mock.setOnPublish { event in
                    let req = try Nip46Request.decryptFromEvent(event, conversationKey: convKey)
                    let call = DecodedCall(id: req.id, method: req.method, params: req.params)
                    await handler(call, helper)
                }
            }
        }
    }
}

/// Holds the mock transport the signer creates inside its factory closure. `onSet` lets
/// the test re-arm publish handlers the moment the mock appears.
final class MockHolder: @unchecked Sendable {
    private let lock = NSLock()
    private var _value: MockRemoteSignerTransport?
    var onSet: ((MockRemoteSignerTransport) -> Void)?

    var value: MockRemoteSignerTransport? {
        lock.lock(); defer { lock.unlock() }
        return _value
    }

    func set(_ mock: MockRemoteSignerTransport) {
        lock.lock(); _value = mock; lock.unlock()
        onSet?(mock)
    }
}

// MARK: - Mock transport

/// Records outbound publishes and lets the test feed inbound frames back through the
/// `onEvent` callback the signer registered at init time.
final actor MockRemoteSignerTransport: RemoteSignerTransport {
    let relayURL: URL
    let sessionPubkeyHex: String
    let bunkerPubkeyHex: String
    private let onEvent: @Sendable (_ senderPubkey: String, _ encryptedContent: String) async -> Void

    private(set) var publishCount: Int = 0
    private(set) var lastEvent: SignedNostrEvent?
    private var _onPublish: (@Sendable (SignedNostrEvent) async throws -> Void)?

    init(
        relayURL: URL,
        sessionPubkeyHex: String,
        bunkerPubkeyHex: String,
        onEvent: @escaping @Sendable (_ senderPubkey: String, _ encryptedContent: String) async -> Void,
        initialOnPublish: (@Sendable (SignedNostrEvent) async throws -> Void)? = nil
    ) {
        self.relayURL = relayURL
        self.sessionPubkeyHex = sessionPubkeyHex
        self.bunkerPubkeyHex = bunkerPubkeyHex
        self.onEvent = onEvent
        self._onPublish = initialOnPublish
    }

    func connect() async {}
    func disconnect() async {}

    func setOnPublish(_ handler: (@Sendable (SignedNostrEvent) async throws -> Void)?) {
        _onPublish = handler
    }

    func publish(event: SignedNostrEvent) async throws {
        publishCount += 1
        lastEvent = event
        if let _onPublish { try await _onPublish(event) }
    }

    func deliverInbound(senderPubkey: String, encryptedContent: String) async {
        await onEvent(senderPubkey, encryptedContent)
    }
}

// MARK: - Helpers

extension Nip46Request {
    /// Decrypt a published kind:24133 event back into a request frame, using the same
    /// conversation key the signer derived. Test-only utility.
    static func decryptFromEvent(_ event: SignedNostrEvent, conversationKey: Data) throws -> Nip46Request {
        let plaintext = try Nip44.decrypt(payload: event.content, conversationKey: conversationKey)
        guard let data = plaintext.data(using: .utf8),
              let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let id = obj["id"] as? String,
              let method = obj["method"] as? String,
              let params = obj["params"] as? [String] else {
            throw NSError(domain: "Nip46Test", code: 0)
        }
        return Nip46Request(id: id, method: method, params: params)
    }
}
