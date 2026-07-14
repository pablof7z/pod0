import Foundation
import Observation
import os.log

/// The human user's Nostr identity — entirely separate from the agent's identity.
/// Manages its own keychain slot and published key state.
///
/// Two flavours of identity are supported:
/// 1. **Local nsec** — a private key stored in the iOS Keychain.
/// 2. **Remote signer (NIP-46)** — a "bunker" connection where the user's nsec lives
///    elsewhere (Amber, nsec.app, nsecBunker, …) and we delegate signing over a relay.
///
/// Call `start()` once at app launch so the store auto-loads any previously saved key
/// or remote-signer connection from the keychain.
@MainActor
@Observable
final class UserIdentityStore {
    private let logger = Logger.app("UserIdentityStore")

    /// The user's signing pubkey (32-byte hex x-only). Always reflects whichever signer
    /// is currently active — local key or NIP-46 user pubkey.
    private(set) var publicKeyHex: String?
    private(set) var keyPair: NostrKeyPair?
    private(set) var loginError: String?

    /// What kind of identity is currently active.
    enum Mode: String, Sendable, Codable {
        case none
        case localKey
        case remoteSigner
    }
    private(set) var mode: Mode = .none

    /// The active signer. Whatever the rest of the app uses to sign events.
    /// `nil` while no identity is configured.
    private(set) var signer: (any NostrSigner)?

    /// Live state of the NIP-46 connection (UI surfaces this).
    private(set) var remoteSignerState: RemoteSignerState = .idle

    /// Cached kind:0 profile fields fetched from relays. `nil` until the
    /// first fetch completes; populated immediately on launch from the
    /// UserDefaults cache so the UI never flashes generated → real.
    var profileDisplayName: String?
    var profileName: String?
    var profileAbout: String?
    var profilePicture: String?

    var hasIdentity: Bool { publicKeyHex != nil }
    var isRemoteSigner: Bool { mode == .remoteSigner }

    // MARK: - Keychain slots

    private static let userKeyService = "\(Bundle.main.bundleIdentifier ?? "Podcastr").user-identity"
    private static let userKeyAccount = "user-private-key-hex"
    private static let userKeyOriginAccount = "user-private-key-origin"
    private static let generatedProfileAccount = "generated-profile-published-pubkey"
    private static let nip46SessionService = "\(Bundle.main.bundleIdentifier ?? "Podcastr").nip46-session"
    private static let nip46SessionAccount = "session-private-key-hex"
    private static let nip46MetaService = "\(Bundle.main.bundleIdentifier ?? "Podcastr").nip46-meta"
    private static let nip46MetaAccount = "connection"
    private static let generatedOrigin = "generated"

    // MARK: - Lifecycle

    func start() {
        // Prefer an existing local key.
        do {
            if let hex = try KeychainStore.readString(service: Self.userKeyService, account: Self.userKeyAccount),
               !hex.isEmpty {
                let pair = try NostrKeyPair(privateKeyHex: hex)
                adoptLocal(pair)
                if isGeneratedLocalKey {
                    publishGeneratedProfileIfNeeded(pair: pair)
                }
                return
            }
        } catch {
            logger.error("UserIdentityStore.start failed to load local key: \(error, privacy: .public)")
        }
        // Otherwise, try to resume a remote-signer connection.
        if let meta = try? loadRemoteMeta(), let session = try? loadSessionKeyPair() {
            publicKeyHex = meta.userPubkeyHex
            mode = .remoteSigner
            remoteSignerState = .reconnecting
            Task { await self.resumeRemote(meta: meta, sessionKeyPair: session) }
        } else {
            do {
                try generateGeneratedKey()
            } catch {
                logger.error("UserIdentityStore.start failed to generate local key: \(error, privacy: .public)")
            }
        }
    }

    // MARK: - nsec import

    func importNsec(_ nsec: String) throws {
        loginError = nil
        let trimmed = nsec.trimmed
        do {
            let pair = try NostrKeyPair(nsec: trimmed)
            try KeychainStore.saveString(pair.privateKeyHex, service: Self.userKeyService, account: Self.userKeyAccount)
            try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyOriginAccount)
            clearRemoteConnectionState()
            adoptLocal(pair)
        } catch {
            loginError = "Invalid nsec — check the key and try again."
            throw error
        }
    }

    // MARK: - Generate ephemeral key

    func generateKey() throws {
        loginError = nil
        do {
            let pair = try NostrKeyPair.generate()
            try KeychainStore.saveString(pair.privateKeyHex, service: Self.userKeyService, account: Self.userKeyAccount)
            try KeychainStore.saveString(Self.generatedOrigin, service: Self.userKeyService, account: Self.userKeyOriginAccount)
            clearRemoteConnectionState()
            adoptLocal(pair)
            publishGeneratedProfileIfNeeded(pair: pair)
        } catch {
            loginError = "Failed to generate key — please try again."
            throw error
        }
    }

    // MARK: - Sign out

    func clearIdentity() {
        do {
            try KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyAccount)
        } catch {
            logger.error("UserIdentityStore.clearIdentity failed: \(error, privacy: .public)")
        }
        try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyOriginAccount)
        try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.generatedProfileAccount)
        Task { await self.tearDownRemote() }
        try? KeychainStore.deleteString(service: Self.nip46SessionService, account: Self.nip46SessionAccount)
        try? KeychainStore.deleteString(service: Self.nip46MetaService, account: Self.nip46MetaAccount)
        keyPair = nil
        publicKeyHex = nil
        signer = nil
        mode = .none
        remoteSignerState = .idle
        profileDisplayName = nil
        profileName = nil
        profileAbout = nil
        profilePicture = nil
    }

    // MARK: - NIP-46 connect / disconnect

    /// Parse a `bunker://…` URI, run the connect handshake, and (on success) persist the
    /// connection so the next app launch reconnects automatically. If the bunker replies
    /// with an `auth_url` challenge, the state advances to `.awaitingAuthorization(url)`
    /// and the UI surfaces a button to open the URL in the browser.
    func connectRemoteSigner(uri: String) async {
        loginError = nil
        let parsed: BunkerURI
        do {
            parsed = try BunkerURI.parse(uri)
        } catch {
            loginError = (error as? LocalizedError)?.errorDescription ?? "Invalid bunker URI."
            remoteSignerState = .failed(loginError ?? "Invalid bunker URI.")
            return
        }
        remoteSignerState = .connecting
        do {
            let sessionPair = try NostrKeyPair.generate()
            let signer = RemoteSigner(bunker: parsed, sessionKeyPair: sessionPair)
            let userPub = try await signer.connect { [weak self] url in
                await self?.handleAuthChallenge(url: url)
            }
            try KeychainStore.saveString(sessionPair.privateKeyHex, service: Self.nip46SessionService, account: Self.nip46SessionAccount)
            try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyAccount)
            try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyOriginAccount)
            let meta = RemoteMeta(
                bunkerPubkeyHex: parsed.remotePubkeyHex,
                relays: parsed.relays,
                secret: parsed.secret,
                permissions: parsed.permissions,
                userPubkeyHex: userPub
            )
            try saveRemoteMeta(meta)
            self.signer = signer
            self.publicKeyHex = userPub
            self.keyPair = nil
            self.mode = .remoteSigner
            self.remoteSignerState = .connected(userPub)
            self.loadCachedProfile(for: userPub)
            let pub = userPub
            Task { await self.fetchAndCacheProfile(pubkeyHex: pub) }
        } catch {
            let msg = (error as? LocalizedError)?.errorDescription ?? "\(error)"
            loginError = msg
            remoteSignerState = .failed(msg)
        }
    }

    /// Surfaces the bunker's `auth_url` URL to the UI. Called from inside `connect(...)`'s
    /// `onAuthChallenge` continuation; the connect call itself is still suspended waiting
    /// for the eventual `ack`.
    private func handleAuthChallenge(url: URL) {
        remoteSignerState = .awaitingAuthorization(url)
    }

    func disconnectRemoteSigner() async {
        await tearDownRemote()
        try? KeychainStore.deleteString(service: Self.nip46SessionService, account: Self.nip46SessionAccount)
        try? KeychainStore.deleteString(service: Self.nip46MetaService, account: Self.nip46MetaAccount)
        publicKeyHex = nil
        keyPair = nil
        signer = nil
        mode = .none
        remoteSignerState = .idle
    }

    // MARK: - Display helpers

    var npub: String? {
        guard let hex = publicKeyHex, let bytes = Data(hexString: hex), bytes.count == 32 else { return nil }
        return Bech32.encode(hrp: "npub", data: bytes)
    }

    var npubShort: String? {
        guard let full = npub, full.count > 16 else { return npub }
        return "\(full.prefix(10))…\(full.suffix(6))"
    }

    // MARK: - Private — local

    private func adoptLocal(_ pair: NostrKeyPair) {
        keyPair = pair
        publicKeyHex = pair.publicKeyHex
        signer = LocalKeySigner(keyPair: pair)
        mode = .localKey
        remoteSignerState = .idle
        loadCachedProfile(for: pair.publicKeyHex)
        guard !isGeneratedLocalKey else { return }
        let pubkey = pair.publicKeyHex
        Task { await self.fetchAndCacheProfile(pubkeyHex: pubkey) }
    }

    private var isGeneratedLocalKey: Bool {
        (try? KeychainStore.readString(
            service: Self.userKeyService,
            account: Self.userKeyOriginAccount
        )) == Self.generatedOrigin
    }

    private func generateGeneratedKey() throws {
        let pair = try NostrKeyPair.generate()
        try KeychainStore.saveString(pair.privateKeyHex, service: Self.userKeyService, account: Self.userKeyAccount)
        try KeychainStore.saveString(Self.generatedOrigin, service: Self.userKeyService, account: Self.userKeyOriginAccount)
        adoptLocal(pair)
        publishGeneratedProfileIfNeeded(pair: pair)
    }

    private func clearRemoteConnectionState() {
        if let remote = signer as? RemoteSigner {
            Task { await remote.disconnect() }
        }
        try? KeychainStore.deleteString(service: Self.nip46SessionService, account: Self.nip46SessionAccount)
        try? KeychainStore.deleteString(service: Self.nip46MetaService, account: Self.nip46MetaAccount)
        remoteSignerState = .idle
    }

    private func publishGeneratedProfileIfNeeded(pair: NostrKeyPair) {
        let alreadyPublished = try? KeychainStore.readString(
            service: Self.userKeyService,
            account: Self.generatedProfileAccount
        )
        guard alreadyPublished != pair.publicKeyHex else { return }
        let pubkey = pair.publicKeyHex
        let keyService = Self.userKeyService
        let profileAccount = Self.generatedProfileAccount
        let signer = LocalKeySigner(keyPair: pair)
        let profile = Self.generatedProfile(pubkey: pubkey)
        Task.detached {
            guard let data = try? JSONSerialization.data(withJSONObject: profile, options: [.sortedKeys]),
                  let content = String(data: data, encoding: .utf8) else { return }
            let event = try await signer.sign(NostrEventDraft(kind: 0, content: content))
            var published = false
            for relayURL in FeedbackRelayClient.profileRelayURLs {
                let client = FeedbackRelayClient(relayURL: relayURL)
                do {
                    try await client.publish(event, authSigner: signer)
                    published = true
                } catch {
                    continue
                }
            }
            if published {
                try? KeychainStore.saveString(
                    pubkey,
                    service: keyService,
                    account: profileAccount
                )
            }
        }
    }

    private static func generatedProfile(pubkey: String) -> [String: String] {
        let seed = String(pubkey.prefix(16))
        let index = stableProfileIndex(seed)
        let adjectives = ["Bright", "Quiet", "Swift", "Kind", "Clear", "North"]
        let nouns = ["Signal", "Notebook", "Harbor", "Lantern", "Thread", "Field"]
        let adjective = adjectives[index % adjectives.count]
        let noun = nouns[(index / adjectives.count) % nouns.count]
        return [
            "name": "\(adjective.lowercased())-\(noun.lowercased())-\(pubkey.prefix(4))",
            "display_name": "\(adjective) \(noun)",
            "about": "Feedback identity generated by Podcastr.",
            "picture": "https://api.dicebear.com/9.x/personas/svg?seed=\(seed)",
        ]
    }

    private static func stableProfileIndex(_ seed: String) -> Int {
        seed.utf8.reduce(0) { partial, byte in
            (partial &* 31 &+ Int(byte)) & 0x7fffffff
        }
    }

    // MARK: - Private — remote

    private func resumeRemote(meta: RemoteMeta, sessionKeyPair: NostrKeyPair) async {
        let bunker = BunkerURI(
            remotePubkeyHex: meta.bunkerPubkeyHex,
            relays: meta.relays,
            secret: meta.secret,
            permissions: meta.permissions
        )
        // Pass the previously-known pubkey so callers of `signer.publicKey()` during the
        // ~1s reconnect window get the cached value instead of `.missingPublicKey`.
        let signer = RemoteSigner(
            bunker: bunker,
            sessionKeyPair: sessionKeyPair,
            cachedUserPublicKeyHex: meta.userPubkeyHex
        )
        // Make the cached signer available to callers immediately (it answers
        // `publicKey()` from cache; sign requests will block on the WebSocket).
        self.signer = signer
        do {
            _ = try await signer.connect { [weak self] url in
                await self?.handleAuthChallenge(url: url)
            }
            self.remoteSignerState = .connected(meta.userPubkeyHex)
        } catch {
            let msg = (error as? LocalizedError)?.errorDescription ?? "\(error)"
            self.remoteSignerState = .failed(msg)
        }
    }

    private func tearDownRemote() async {
        if let s = signer as? RemoteSigner { await s.disconnect() }
    }

    private func loadSessionKeyPair() throws -> NostrKeyPair? {
        guard let hex = try KeychainStore.readString(service: Self.nip46SessionService, account: Self.nip46SessionAccount),
              !hex.isEmpty else { return nil }
        return try NostrKeyPair(privateKeyHex: hex)
    }

    private func loadRemoteMeta() throws -> RemoteMeta? {
        guard let json = try KeychainStore.readString(service: Self.nip46MetaService, account: Self.nip46MetaAccount),
              let data = json.data(using: .utf8) else { return nil }
        return try JSONDecoder().decode(RemoteMeta.self, from: data)
    }

    private func saveRemoteMeta(_ meta: RemoteMeta) throws {
        let data = try JSONEncoder().encode(meta)
        guard let s = String(data: data, encoding: .utf8) else { return }
        try KeychainStore.saveString(s, service: Self.nip46MetaService, account: Self.nip46MetaAccount)
    }

    // MARK: - Process-wide singleton (slice B)

    /// Process-wide instance used by the wiring layer (`AppStateStore+Notes`,
    /// `AppStateStore+Clips`, `EditProfileView.save`) so domain-level
    /// extensions can publish without DI threading. The SwiftUI environment
    /// instance in `AppMain` references the same object so observation stays
    /// consistent.
    static let shared = UserIdentityStore()

    // MARK: - Slice-B internal helpers

    /// Internal alias for the file-private `generateGeneratedKey()` so the
    /// extension file `UserIdentityStore+Publishing.swift` can self-heal
    /// when invoked without an active signer.
    func _ensureGeneratedKey() throws {
        try generateGeneratedKey()
    }

    func _beginNostrConnect() {
        loginError = nil
        remoteSignerState = .connecting
    }

    func _failNostrConnect(_ message: String) {
        loginError = message
        remoteSignerState = .failed(message)
    }

    /// Called by `UserIdentityStore+NIP46.swift` after nostrconnect pairing completes.
    /// Persists the session + meta and updates published identity state.
    func _adoptNostrConnectSigner(
        signer: RemoteSigner,
        userPubkeyHex: String,
        sessionPrivKeyHex: String,
        relayAbsoluteString: String
    ) throws {
        let meta = RemoteMeta(
            bunkerPubkeyHex: signer.bunker.remotePubkeyHex,
            relays: [relayAbsoluteString],
            secret: nil,
            permissions: [],
            userPubkeyHex: userPubkeyHex
        )
        try KeychainStore.saveString(sessionPrivKeyHex, service: Self.nip46SessionService, account: Self.nip46SessionAccount)
        try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyAccount)
        try? KeychainStore.deleteString(service: Self.userKeyService, account: Self.userKeyOriginAccount)
        try saveRemoteMeta(meta)
        self.signer = signer
        self.publicKeyHex = userPubkeyHex
        self.keyPair = nil
        self.mode = .remoteSigner
        self.remoteSignerState = .connected(userPubkeyHex)
    }

    // MARK: - Test seam (slice B)

    /// Test-only: swap in a recording signer so wiring tests can verify
    /// which call-sites reach the signer without standing up a real keypair.
    /// Production code never calls this. Lives in the core file because
    /// `signer` / `publicKeyHex` / `mode` use file-private `private(set)`.
    func _setSignerForTesting(_ signer: any NostrSigner, publicKeyHex: String = String(repeating: "0", count: 64)) {
        self.signer = signer
        self.publicKeyHex = publicKeyHex
        self.mode = .localKey
    }

    /// Test-only: drop the active signer so tests can verify the
    /// `signer == nil` → `generateGeneratedKey()` self-heal path.
    func _clearSignerForTesting() {
        self.signer = nil
        self.publicKeyHex = nil
        self.mode = .none
    }
}

// MARK: - Supporting types

/// Connection state surfaced to the UI for the NIP-46 flow.
enum RemoteSignerState: Sendable, Equatable {
    case idle
    case connecting
    case reconnecting
    /// The bunker replied with an `auth_url` challenge — the user must approve in a
    /// browser. The connect call itself is still suspended; `connected(...)` follows
    /// once the bunker delivers the real `ack`.
    case awaitingAuthorization(URL)
    case connected(String)            // associated value: user pubkey hex
    case failed(String)               // error message
}

/// Minimal persisted NIP-46 metadata. Stored in the Keychain (JSON) alongside the
/// session private key so the app can resume on launch without prompting for the URI again.
private struct RemoteMeta: Codable, Sendable {
    let bunkerPubkeyHex: String
    let relays: [String]
    let secret: String?
    let permissions: [String]
    let userPubkeyHex: String
}

enum UserIdentityError: LocalizedError {
    case noIdentity

    var errorDescription: String? {
        switch self {
        case .noIdentity:
            "No feedback identity is available."
        }
    }
}
