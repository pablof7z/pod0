import CryptoKit
import Foundation
import os.log

// MARK: - nostrconnect:// pairing flow (NIP-46 client-initiated)
//
// Distinct from the bunker:// flow where the signer hands us its URI.
// Here the *client* generates the URI (QR / deep-link), advertises it, and
// waits for the signer to call back over the relay channel.
//
// Flow:
//   1. Generate ephemeral secret + session keypair.
//   2. Build nostrconnect:// URI and surface it via `onURI`.
//   3. Subscribe on the relay with NO `authors` filter (signer pubkey unknown).
//   4. For each inbound kind:24133 event, try NIP-44 decryption with
//      (sessionPrivKey, senderPubkey). Parse result. If result == secret: found.
//   5. Tear down discovery transport. Build RemoteSigner with discovered pubkey.
//   6. Call `finishNostrConnect` — only runs `get_public_key` (no `connect` RPC).

extension RemoteSigner {

    private static let logger = Logger.app("RemoteSigner+NC")

    /// Default relay used for nostrconnect pairing. Primal's relay is a common
    /// choice in the ecosystem (Olas, highlighter all use it as the default).
    static let nostrConnectDefaultRelay = URL(string: "wss://relay.primal.net")!

    /// Default permissions requested from signer apps during nostrconnect pairing.
    static let nostrConnectDefaultPerms =
        "sign_event:1,sign_event:6,sign_event:7,nip44_encrypt,nip44_decrypt"

    // MARK: - Factory

    /// Perform the nostrconnect:// inbound-pairing flow and return a live signer.
    ///
    /// - Parameters:
    ///   - relayURL: Relay to use for the pairing channel. Defaults to Primal.
    ///   - sessionKeyPair: Ephemeral keypair that acts as the kind:24133 author.
    ///   - appName: Human-readable name sent in the URI (displayed by signer apps).
    ///   - permsString: Comma-separated permissions string; nil → default set.
    ///   - onURI: Called synchronously with the generated nostrconnect:// URI so
    ///     the UI can render a QR code or open a signer deep-link immediately.
    ///   - timeout: How long to wait before giving up. Default 5 minutes.
    ///   - transportFactory: Override for tests.
    /// - Returns: `(signer, userPubkeyHex)` ready for signing.
    static func nostrConnect(
        relayURL: URL = nostrConnectDefaultRelay,
        sessionKeyPair: NostrKeyPair,
        appName: String = "Podcastr",
        permsString: String? = nil,
        onURI: @escaping @Sendable (String) -> Void,
        timeout: Duration = .seconds(300),
        transportFactory: @escaping RemoteSignerTransportFactory = defaultRemoteSignerTransportFactory
    ) async throws -> (RemoteSigner, String) {
        let secret = generateNostrConnectSecret()
        let uri = buildNostrConnectURI(
            relay: relayURL,
            sessionPubkeyHex: sessionKeyPair.publicKeyHex,
            secret: secret,
            appName: appName,
            permsString: permsString ?? nostrConnectDefaultPerms
        )
        onURI(uri)

        let bunkerPubkeyHex = try await awaitInboundSecret(
            relayURL: relayURL,
            sessionKeyPair: sessionKeyPair,
            secret: secret,
            timeout: timeout,
            transportFactory: transportFactory
        )
        Self.logger.info("nostrconnect: discovered signer pubkey \(bunkerPubkeyHex.prefix(12), privacy: .public)…")

        let bunker = BunkerURI(
            remotePubkeyHex: bunkerPubkeyHex,
            relays: [relayURL.absoluteString],
            secret: nil,
            permissions: []
        )
        let signer = RemoteSigner(
            bunker: bunker,
            sessionKeyPair: sessionKeyPair,
            transportFactory: transportFactory
        )
        let userPub = try await signer.finishNostrConnect(relayURL: relayURL)
        return (signer, userPub)
    }

    // MARK: - URI builder

    static func buildNostrConnectURI(
        relay: URL,
        sessionPubkeyHex: String,
        secret: String,
        appName: String,
        permsString: String
    ) -> String {
        var comps = URLComponents()
        comps.scheme = "nostrconnect"
        comps.host = sessionPubkeyHex
        var items: [URLQueryItem] = [
            URLQueryItem(name: "relay", value: relay.absoluteString),
            URLQueryItem(name: "secret", value: secret),
            URLQueryItem(name: "name", value: appName),
        ]
        if !permsString.isEmpty {
            items.append(URLQueryItem(name: "perms", value: permsString))
        }
        comps.queryItems = items
        return comps.url?.absoluteString ?? "nostrconnect://\(sessionPubkeyHex)"
    }

    // MARK: - Inbound secret listener

    /// Opens a discovery transport (no `authors` filter) and blocks until an inbound
    /// kind:24133 event arrives that decrypts under our session key AND contains
    /// `result == secret`. Returns the sender's pubkey (= the signer's pubkey).
    private static func awaitInboundSecret(
        relayURL: URL,
        sessionKeyPair: NostrKeyPair,
        secret: String,
        timeout: Duration,
        transportFactory: RemoteSignerTransportFactory
    ) async throws -> String {
        try await withThrowingTaskGroup(of: String.self) { group in
            let stream = AsyncThrowingStream<String, Error> { continuation in
                let transport = transportFactory(relayURL, sessionKeyPair.publicKeyHex, nil) { sender, encrypted in
                    guard let convKey = try? Nip44.conversationKey(
                        privateKeyHex: sessionKeyPair.privateKeyHex,
                        peerPublicKeyHex: sender
                    ),
                    let plaintext = try? Nip44.decrypt(payload: encrypted, conversationKey: convKey),
                    let response = try? Nip46Response.parse(plaintext),
                    response.result == secret
                    else { return }
                    continuation.yield(sender)
                    continuation.finish()
                }
                Task {
                    await transport.connect()
                }
                continuation.onTermination = { _ in
                    Task { await transport.disconnect() }
                }
            }

            group.addTask {
                for try await senderPubkey in stream { return senderPubkey }
                throw NostrSignerError.timedOut
            }
            group.addTask {
                try await Task.sleep(for: timeout)
                throw NostrSignerError.timedOut
            }

            let result = try await group.next()!
            group.cancelAll()
            return result
        }
    }

    // MARK: - Helpers

    private static func generateNostrConnectSecret() -> String {
        var bytes = [UInt8](repeating: 0, count: 16)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return bytes.map { String(format: "%02x", $0) }.joined()
    }
}
