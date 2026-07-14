import Foundation
import os.log

/// `NostrSigner` backed by a NIP-46 remote signer ("bunker"). All signing is delegated
/// over an encrypted kind:24133 channel; the bunker holds the user's nsec.
///
/// Lifecycle:
/// 1. `connect(onAuthChallenge:)` — opens relay, sends `connect` RPC, awaits `ack`, then
///    `get_public_key` to learn the user-pubkey the bunker signs as. The bunker may
///    interleave one or more `auth_url` challenges before ack — see the spec's "Auth
///    Challenges" section. The supplied callback is invoked once per URL so the UI can
///    open it; the original continuation keeps waiting for the real ack.
/// 2. `sign(_:)` — sends `sign_event` with the canonical event JSON, awaits the signed
///    event JSON in the response.
///
/// Multi-relay: a `bunker://` URI may list several relays. `connect` walks them in order
/// and stops at the first one that completes the handshake; subsequent traffic uses only
/// that relay (no fan-out, to keep request/response correlation simple).
actor RemoteSigner: NostrSigner {
    private static let logger = Logger.app("RemoteSigner")

    let bunker: BunkerURI
    /// Ephemeral session keypair we use as the kind:24133 author. Never the user's key.
    let sessionKeyPair: NostrKeyPair

    /// Production: opens a real WebSocket. Tests inject a mock.
    private let transportFactory: RemoteSignerTransportFactory

    private var transport: (any RemoteSignerTransport)?
    private var conversationKey: Data?
    /// Cached so callers during the brief reconnect window after an app restart can still
    /// observe the user pubkey (we already loaded it from persisted Keychain meta on boot,
    /// so failing publicKey() with `.missingPublicKey` would just be needless flapping).
    /// Staleness window: from process start until `connect()` finishes a new handshake.
    /// During that window the cached value reflects the *previous* successful session.
    private var userPublicKeyHex: String?

    /// The relay we're currently connected on (set after a successful handshake). The
    /// other URIs from `bunker.relays` are tried in order and discarded on failure.
    private(set) var activeRelayURL: URL?

    /// Outstanding requests, keyed by request id. `auth_url` responses do **not** consume
    /// the continuation — we keep waiting for the real ack on the same id.
    private var pending: [String: CheckedContinuation<Nip46Response, Error>] = [:]
    /// Per-request timeout task. Stored separately so we can cancel + reschedule when an
    /// `auth_url` challenge arrives (the user's browser flow blows past 30s easily).
    private var timeouts: [String: Task<Void, Never>] = [:]

    /// Per-request timeout for normal RPCs. Bumped to `authChallengeTimeout` once an
    /// `auth_url` challenge has been observed for that id.
    private let requestTimeout: Duration
    /// Window we give the user to click through a browser auth flow before giving up.
    private let authChallengeTimeout: Duration

    init(
        bunker: BunkerURI,
        sessionKeyPair: NostrKeyPair,
        cachedUserPublicKeyHex: String? = nil,
        requestTimeout: Duration = .seconds(30),
        authChallengeTimeout: Duration = .seconds(180),
        transportFactory: @escaping RemoteSignerTransportFactory = defaultRemoteSignerTransportFactory
    ) {
        self.bunker = bunker
        self.sessionKeyPair = sessionKeyPair
        self.userPublicKeyHex = cachedUserPublicKeyHex
        self.requestTimeout = requestTimeout
        self.authChallengeTimeout = authChallengeTimeout
        self.transportFactory = transportFactory
    }

    // MARK: - NostrSigner

    /// Returns the user pubkey if we know it — either from a completed handshake **or**
    /// from a cache passed in at init time (e.g. persisted Keychain meta during reconnect).
    func publicKey() async throws -> String {
        if let pk = userPublicKeyHex { return pk }
        throw NostrSignerError.missingPublicKey
    }

    func sign(_ draft: NostrEventDraft) async throws -> SignedNostrEvent {
        guard let userPub = userPublicKeyHex else { throw NostrSignerError.missingPublicKey }
        // Build canonical event JSON (without `id`/`sig`) for the bunker. Most bunkers
        // accept the unsigned event as a JSON-stringified object.
        let unsigned: [String: Any] = [
            "pubkey": userPub,
            "created_at": draft.createdAt,
            "kind": draft.kind,
            "tags": draft.tags,
            "content": draft.content,
        ]
        let data = try JSONSerialization.data(withJSONObject: unsigned, options: [])
        guard let payload = String(data: data, encoding: .utf8) else {
            throw NostrSignerError.invalidEventForSigning
        }
        let req = Nip46Request(method: .signEvent, params: [payload])
        let resp = try await call(req)
        guard let signedJSON = resp.result?.data(using: .utf8) else {
            throw NostrSignerError.remoteRejected("Empty `result` from bunker on sign_event")
        }
        return try JSONDecoder().decode(SignedNostrEvent.self, from: signedJSON)
    }

    // MARK: - Connect handshake

    /// Walk the bunker's relay list in order and run `connect` + `get_public_key` on the
    /// first relay that succeeds. `onAuthChallenge` is invoked (potentially more than once)
    /// when the bunker responds with `result == "auth_url"`; the URL is the page the user
    /// must approve in a browser. The connect call itself blocks until the bunker sends
    /// the real `ack` (or we exhaust `authChallengeTimeout`).
    func connect(onAuthChallenge: (@Sendable (URL) async -> Void)? = nil) async throws -> String {
        var lastError: Error?
        for relayString in bunker.relays {
            guard let url = URL(string: relayString) else { continue }
            do {
                let pubkey = try await runHandshake(on: url, onAuthChallenge: onAuthChallenge)
                Self.logger.info("RemoteSigner: connected via \(url.absoluteString, privacy: .public)")
                return pubkey
            } catch {
                Self.logger.warning("RemoteSigner: handshake on \(url.absoluteString, privacy: .public) failed — \(error, privacy: .public)")
                await teardownTransport()
                lastError = error
                continue
            }
        }
        throw lastError ?? NostrSignerError.notConnected
    }

    /// Open one relay and run the full `connect` + `get_public_key` exchange against it.
    private func runHandshake(on url: URL, onAuthChallenge: (@Sendable (URL) async -> Void)?) async throws -> String {
        let convKey = try Nip44.conversationKey(
            privateKeyHex: sessionKeyPair.privateKeyHex,
            peerPublicKeyHex: bunker.remotePubkeyHex
        )
        self.conversationKey = convKey

        let inbox = self
        let transport = transportFactory(url, sessionKeyPair.publicKeyHex, bunker.remotePubkeyHex) { [weak inbox] sender, encrypted in
            await inbox?.handleIncoming(senderPubkey: sender, encryptedContent: encrypted)
        }
        await transport.connect()
        self.transport = transport
        self.activeRelayURL = url

        // 1. connect — params: [remote_pubkey, secret?, perms?]
        var params = [bunker.remotePubkeyHex]
        if let secret = bunker.secret { params.append(secret) }
        if !bunker.permissions.isEmpty { params.append(bunker.permissions.joined(separator: ",")) }
        let connectResp = try await call(Nip46Request(method: .connect, params: params), authChallengeHandler: onAuthChallenge)
        if let err = connectResp.error, !err.isEmpty {
            throw NostrSignerError.remoteRejected("connect: \(err)")
        }

        // 2. get_public_key — learn the user's signing pubkey.
        let pkResp = try await call(Nip46Request(method: .getPublicKey, params: []))
        if let err = pkResp.error, !err.isEmpty {
            throw NostrSignerError.remoteRejected("get_public_key: \(err)")
        }
        guard let pk = pkResp.result, isValidHex(pk, length: 64) else {
            throw NostrSignerError.remoteRejected("get_public_key returned an unparseable pubkey")
        }
        self.userPublicKeyHex = pk
        return pk
    }

    /// Called after nostrconnect pairing: the signer already sent `result == secret` (that
    /// served as the implicit `connect` ack). Skip the `connect` RPC and go straight to
    /// `get_public_key` so no second auth-challenge is issued.
    func finishNostrConnect(relayURL: URL) async throws -> String {
        let convKey = try Nip44.conversationKey(
            privateKeyHex: sessionKeyPair.privateKeyHex,
            peerPublicKeyHex: bunker.remotePubkeyHex
        )
        self.conversationKey = convKey
        let inbox = self
        let transport = transportFactory(relayURL, sessionKeyPair.publicKeyHex, bunker.remotePubkeyHex) { [weak inbox] sender, encrypted in
            await inbox?.handleIncoming(senderPubkey: sender, encryptedContent: encrypted)
        }
        await transport.connect()
        self.transport = transport
        self.activeRelayURL = relayURL
        let pkResp = try await call(Nip46Request(method: .getPublicKey, params: []))
        if let err = pkResp.error, !err.isEmpty {
            throw NostrSignerError.remoteRejected("get_public_key: \(err)")
        }
        guard let pk = pkResp.result, isValidHex(pk, length: 64) else {
            throw NostrSignerError.remoteRejected("get_public_key returned an unparseable pubkey")
        }
        self.userPublicKeyHex = pk
        return pk
    }

    func disconnect() async {
        await teardownTransport()
        for t in timeouts.values { t.cancel() }
        timeouts.removeAll()
        for (_, c) in pending { c.resume(throwing: NostrSignerError.notConnected) }
        pending.removeAll()
    }

    private func teardownTransport() async {
        if let t = transport { await t.disconnect() }
        transport = nil
        activeRelayURL = nil
    }

    // MARK: - Inbound dispatch

    /// Decrypt an inbound kind:24133 event and route it to the matching pending request.
    /// Special-case: a response whose `result == "auth_url"` keeps the continuation alive,
    /// extends the timeout, and forwards the URL (from the `error` field per the spec) to
    /// the registered auth-challenge handler.
    private func handleIncoming(senderPubkey: String, encryptedContent: String) async {
        guard senderPubkey == bunker.remotePubkeyHex, let convKey = conversationKey else { return }
        guard let plaintext = try? Nip44.decrypt(payload: encryptedContent, conversationKey: convKey) else {
            Self.logger.warning("RemoteSigner: failed to decrypt incoming kind:24133")
            return
        }
        guard let response = try? Nip46Response.parse(plaintext) else { return }

        if response.result == "auth_url" {
            await handleAuthChallenge(for: response)
            return
        }
        if let cont = pending.removeValue(forKey: response.id) {
            timeouts.removeValue(forKey: response.id)?.cancel()
            cont.resume(returning: response)
        } else {
            Self.logger.debug("RemoteSigner: dropping unmatched response id \(response.id, privacy: .public)")
        }
    }

    /// Per spec: `result == "auth_url"`, `error == <URL>`. Don't resume the original
    /// continuation — extend the deadline so the user has time to click through.
    private func handleAuthChallenge(for response: Nip46Response) async {
        guard let urlString = response.error, let url = URL(string: urlString) else {
            Self.logger.warning("RemoteSigner: auth_url response missing URL in error field")
            return
        }
        guard pending[response.id] != nil else {
            Self.logger.debug("RemoteSigner: auth_url for unknown request id — ignoring")
            return
        }
        Self.logger.info("RemoteSigner: bunker requires browser auth — extending deadline")
        // Reset the timeout to the (much longer) auth-challenge window.
        rescheduleTimeout(for: response.id, after: authChallengeTimeout)
        if let handler = authChallengeHandler {
            Task { await handler(url) }
        }
    }

    // MARK: - Send + await

    /// Currently-active auth-challenge callback. Set per-call inside `call(_:authChallengeHandler:)`
    /// so it only fires for the in-flight `connect` request.
    private var authChallengeHandler: (@Sendable (URL) async -> Void)?

    private func call(
        _ request: Nip46Request,
        authChallengeHandler handler: (@Sendable (URL) async -> Void)? = nil
    ) async throws -> Nip46Response {
        guard let convKey = conversationKey else { throw NostrSignerError.notConnected }
        let json = try request.encode()
        let ciphertext = try Nip44.encrypt(plaintext: json, conversationKey: convKey)
        let draft = NostrEventDraft(
            kind: 24133,
            content: ciphertext,
            tags: [["p", bunker.remotePubkeyHex]]
        )
        // Sign with the **session** key (bunker authenticates us via this).
        let signed = try await LocalKeySigner(keyPair: sessionKeyPair).sign(draft)
        guard let transport else { throw NostrSignerError.notConnected }

        let previousHandler = authChallengeHandler
        self.authChallengeHandler = handler
        defer { self.authChallengeHandler = previousHandler }

        return try await registerAndAwait(id: request.id) {
            try await transport.publish(event: signed)
        }
    }

    /// Register a continuation under `id`, publish the encrypted event, and start the
    /// timeout task. Cleans up `pending[id]` and `timeouts[id]` on every exit path.
    private func registerAndAwait(
        id: String,
        send: @escaping @Sendable () async throws -> Void
    ) async throws -> Nip46Response {
        let timeout = self.requestTimeout
        return try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Nip46Response, Error>) in
            pending[id] = continuation
            scheduleTimeout(for: id, after: timeout)
            Task { [weak self] in
                do {
                    try await send()
                } catch {
                    await self?.failPending(id: id, error: error)
                }
            }
        }
    }

    private func scheduleTimeout(for id: String, after duration: Duration) {
        timeouts[id]?.cancel()
        timeouts[id] = Task { [weak self] in
            try? await Task.sleep(for: duration)
            if Task.isCancelled { return }
            await self?.failPending(id: id, error: NostrSignerError.timedOut)
        }
    }

    private func rescheduleTimeout(for id: String, after duration: Duration) {
        guard pending[id] != nil else { return }
        scheduleTimeout(for: id, after: duration)
    }

    /// If a continuation for `id` is still pending, resolve it with `error` and remove it.
    /// No-op if the response already arrived (continuation was consumed in `handleIncoming`).
    private func failPending(id: String, error: Error) {
        timeouts.removeValue(forKey: id)?.cancel()
        guard let cont = pending.removeValue(forKey: id) else { return }
        cont.resume(throwing: error)
    }

    // MARK: - Helpers

    private func isValidHex(_ s: String, length: Int) -> Bool {
        guard s.count == length else { return false }
        return s.allSatisfy { ($0 >= "0" && $0 <= "9") || ($0 >= "a" && $0 <= "f") || ($0 >= "A" && $0 <= "F") }
    }
}
