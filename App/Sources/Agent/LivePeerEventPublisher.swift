import Foundation
import os.log

// MARK: - LivePeerEventPublisher
//
// Live wiring for `PeerEventPublisherProtocol`. Signs kind:1 events with the
// user's agent key (when available) and pushes them to the configured relay
// via the shared `NostrEventPublishing` one-shot publisher. Used by the
// `end_conversation` (final-message branch) and `send_friend_message` agent
// tools.

final class LivePeerEventPublisher: PeerEventPublisherProtocol, @unchecked Sendable {
    private let logger = Logger.app("AgentTools")
    weak var store: AppStateStore?
    private let publisher: NostrEventPublishing

    init(
        store: AppStateStore,
        publisher: NostrEventPublishing = NostrWebSocketEventPublisher()
    ) {
        self.store = store
        self.publisher = publisher
    }

    func publishConversationReply(
        peerContext: PeerConversationContext,
        body: String,
        extraTags: [[String]]
    ) async throws -> String {
        let trimmed = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            throw NostrEventPublisherError.encodingFailed
        }
        var tags: [[String]] = []
        for a in peerContext.rootATags { tags.append(a) }
        tags.append(["e", peerContext.rootEventID, "", "root"])
        if peerContext.inboundEventID != peerContext.rootEventID {
            tags.append(["e", peerContext.inboundEventID, "", "reply"])
        }
        tags.append(["p", peerContext.peerPubkeyHex])
        tags.append(contentsOf: extraTags)

        return try await signAndPublish(content: trimmed, tags: tags)
    }

    func publishFriendMessage(
        friendPubkeyHex: String,
        body: String,
        peerContext: PeerConversationContext?
    ) async throws -> String {
        let trimmed = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            throw NostrEventPublisherError.encodingFailed
        }
        var tags: [[String]] = []
        // Copy a-tags from the peer context (channel anchors) or fall back to
        // the project coordinate. No e-tags: friend messages are always root
        // events so the friend's response can be detected via NIP-10 root lookup.
        if let peerContext {
            for a in peerContext.rootATags { tags.append(a) }
        } else {
            tags.append(["a", FeedbackRelayClient.projectCoordinate])
        }
        tags.append(["p", friendPubkeyHex])
        return try await signAndPublish(content: trimmed, tags: tags)
    }

    // MARK: - Helpers

    private func signAndPublish(content: String, tags: [[String]]) async throws -> String {
        guard let key = try NostrCredentialStore.privateKey() else {
            throw NostrEventPublisherError.noSigningKey
        }
        let pair = try NostrKeyPair(privateKeyHex: key)
        let createdAt = Int(Date().timeIntervalSince1970)
        let draft = NostrEventDraft(kind: 1, content: content, tags: tags, createdAt: createdAt)
        let signed = try await LocalKeySigner(keyPair: pair).sign(draft)

        let settings = await MainActor.run { store?.state.settings }
        guard settings?.nostrEnabled == true,
              let relayString = settings?.nostrRelayURL.trimmed, !relayString.isEmpty,
              let relayURL = URL(string: relayString) else {
            throw NostrEventPublisherError.noRelayConfigured
        }
        try await publisher.publish(event: signed, relayURL: relayURL)
        logger.info("LivePeerEventPublisher: published kind:1 event \(signed.id, privacy: .public)")
        return signed.id
    }
}

// MARK: - LiveFriendDirectoryAdapter

/// Resolves the user's friends list from `AppStateStore` for the
/// `send_friend_message` tool's authorization gate.
struct LiveFriendDirectoryAdapter: FriendDirectoryProtocol {
    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func resolvePubkey(prefixOrFull: String) async -> String? {
        let needle = prefixOrFull.lowercased()
        let friends: [Friend] = await MainActor.run { store?.state.friends ?? [] }
        return friends.first { $0.identifier.lowercased().hasPrefix(needle) }?.identifier
    }
}

// MARK: - LivePendingFriendMessageRegistrar

/// Routes a `PendingFriendMessage` into `AppStateStore` from the `async`
/// tool dispatch context. Uses `@unchecked Sendable` because the `store`
/// reference is only ever touched through `MainActor.run`.
final class LivePendingFriendMessageRegistrar: PendingFriendMessageRegistrarProtocol, @unchecked Sendable {
    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func register(_ message: PendingFriendMessage) async {
        await MainActor.run { [weak self] in
            self?.store?.registerPendingFriendMessage(message)
        }
    }
}
