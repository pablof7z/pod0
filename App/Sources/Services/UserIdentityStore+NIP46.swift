import Foundation

// MARK: - UserIdentityStore nostrconnect flow
//
// Keeps nostrconnect pairing logic out of the 500-line-capped core file.
// Uses the internal seams added to the main file.

extension UserIdentityStore {

    /// Begin nostrconnect:// pairing. Generates the nostrconnect URI and calls
    /// `onURI` synchronously so the UI can display the QR code or open a signer
    /// app immediately. Blocks until pairing completes or the 5-minute timeout
    /// expires. On success the identity switches to `.remoteSigner` and the
    /// connection is persisted for automatic reconnect on next launch.
    func connectViaNostrConnect(
        relay: URL = RemoteSigner.nostrConnectDefaultRelay,
        onURI: @escaping @Sendable (String) -> Void
    ) async {
        _beginNostrConnect()
        do {
            let sessionPair = try NostrKeyPair.generate()
            let (signer, userPub) = try await RemoteSigner.nostrConnect(
                relayURL: relay,
                sessionKeyPair: sessionPair,
                onURI: onURI
            )
            try _adoptNostrConnectSigner(
                signer: signer,
                userPubkeyHex: userPub,
                sessionPrivKeyHex: sessionPair.privateKeyHex,
                relayAbsoluteString: relay.absoluteString
            )
            loadCachedProfile(for: userPub)
            let pub = userPub
            Task { await self.fetchAndCacheProfile(pubkeyHex: pub) }
        } catch {
            let msg = (error as? LocalizedError)?.errorDescription ?? "\(error)"
            _failNostrConnect(msg)
        }
    }
}
