import Foundation
import os.log

extension Notification.Name {
    /// Posted on the main thread after a headless in-app delegation run completes.
    /// `object` is the `UUID` of the conversation that was updated.
    static let agentDelegationDidComplete = Notification.Name("agentDelegationDidComplete")
}

// MARK: - Delegation response handling
//
// When `send_friend_message` fires, a `PendingFriendMessage` is registered
// with the store. When the friend's reply arrives, `process(_:)` detects the
// match via NIP-10 root lookup, claims the entry, and dispatches here. This
// file owns the two re-invocation paths:
//
//   • `.inAppChat`  — headless `AgentChatSession` run on the stored conversation.
//   • `.nostrPeer`  — `AgentRelayBridge` run injecting the response into the
//                     original peer-conversation thread, followed by a Nostr publish.

extension NostrAgentResponder {

    /// Re-invokes the originating conversation with the friend's response
    /// injected as a user message. Records the inbound delegation turn.
    func handleDelegationResponse(
        inbound: Inbound,
        pending: PendingFriendMessage
    ) async {
        guard let store else { return }

        let friendLabel = "agent \(pending.friendPubkey.prefix(8))"
        let injectedContent = "[Delegation response from \(friendLabel)]: \(inbound.content)"

        // Record the incoming delegation turn under the pending message's root.
        let turn = NostrConversationTurn(
            eventID: inbound.eventID,
            direction: .incoming,
            pubkey: inbound.pubkey,
            createdAt: Date(timeIntervalSince1970: TimeInterval(inbound.createdAt)),
            content: inbound.content,
            rawEventJSON: inbound.rawEventJSON
        )
        store.recordNostrTurn(
            rootEventID: pending.sentEventID,
            turn: turn,
            counterpartyPubkey: inbound.pubkey
        )
        store.noteNostrActivity(counterpartyPubkey: inbound.pubkey)

        switch pending.origin {
        case .inAppChat(let conversationID):
            await reinvokeInAppChat(
                conversationID: conversationID,
                injectedContent: injectedContent
            )

        case .nostrPeer(let peerRootEventID, let peerPubkey):
            await reinvokeNostrPeer(
                peerRootEventID: peerRootEventID,
                peerPubkey: peerPubkey,
                injectedContent: injectedContent,
                friendPubkey: inbound.pubkey
            )
        }
    }

    // MARK: - In-app chat re-invocation

    private func reinvokeInAppChat(
        conversationID: UUID,
        injectedContent: String
    ) async {
        guard let store else { return }
        Self.logger.notice(
            "delegation: re-invoking in-app chat \(conversationID, privacy: .public)"
        )
        let session = AgentChatSession(
            store: store,
            podcastDeps: podcastDepsProvider?(),
            history: .shared,
            resumeWindow: 0,
            askCoordinator: askCoordinator,
            drainPendingContext: false
        )
        await session.switchToConversation(conversationID)
        await session.send(injectedContent, source: .nostrInbound)
        NotificationCenter.default.post(name: .agentDelegationDidComplete, object: conversationID)
        Self.logger.notice("delegation: in-app chat re-invocation complete")
    }

    // MARK: - Nostr peer re-invocation

    private func reinvokeNostrPeer(
        peerRootEventID: String,
        peerPubkey: String,
        injectedContent: String,
        friendPubkey: String
    ) async {
        guard let store else { return }
        guard let relayURL = URL(string: store.state.settings.nostrRelayURL),
              !store.state.settings.nostrRelayURL.isEmpty else {
            Self.logger.error("delegation: no relay URL; cannot re-invoke nostrPeer")
            return
        }

        // Fetch the original peer thread so the agent has full context.
        let priorEvents = await NostrThreadFetcher.fetch(
            rootID: peerRootEventID,
            relayURL: relayURL
        )
        let selfHex = store.state.settings.nostrPublicKeyHex ?? ""

        // Build message history from the thread, then append the injection.
        var history: [[String: Any]] = priorEvents
            .sorted { $0.createdAt < $1.createdAt }
            .map { ev in
                if ev.pubkey == selfHex {
                    return ["role": "assistant", "content": ev.content]
                }
                let label = NostrPeerAgentPrompt.peerLabel(for: ev.pubkey, in: store)
                let truncated = NostrPeerAgentPrompt.truncatedNpub(fromHex: ev.pubkey)
                let sanitized = NostrPeerAgentPrompt.stripFromPrefix(ev.content)
                return ["role": "user", "content": "[from \(label) (\(truncated))]: \(sanitized)"]
            }
        history.append(["role": "user", "content": injectedContent])

        let bridge = AgentRelayBridge(
            store: store,
            podcastDeps: podcastDepsProvider?(),
            askCoordinator: askCoordinator
        )
        let replyText = await bridge.reply(
            messages: history,
            peerPubkey: peerPubkey,
            rootEventID: peerRootEventID,
            inboundEventID: peerRootEventID
        ) ?? ""
        guard !replyText.isEmpty else {
            Self.logger.notice("delegation: nostrPeer re-invocation produced no reply text; skipping publish")
            return
        }

        // Sign and publish the reply in the original peer thread.
        let privateKey: String?
        do { privateKey = try NostrCredentialStore.privateKey() } catch {
            Self.logger.error("delegation: keychain read failed — \(error, privacy: .public)")
            return
        }
        guard let privKey = privateKey,
              let keyPair = try? NostrKeyPair(privateKeyHex: privKey) else {
            Self.logger.notice("delegation: no local private key; cannot publish nostrPeer reply")
            return
        }

        var replyTags: [[String]] = [
            ["e", peerRootEventID, "", "root"],
            ["p", peerPubkey],
        ]
        // Copy a-tags from the thread root if available.
        if let rootEvent = priorEvents.first(where: { $0.id == peerRootEventID }) {
            let aTags = rootEvent.tags.filter { $0.first == "a" }
            replyTags = aTags + replyTags
        }

        let draft = NostrEventDraft(kind: 1, content: replyText, tags: replyTags)
        let signed: SignedNostrEvent
        do {
            signed = try await LocalKeySigner(keyPair: keyPair).sign(draft)
        } catch {
            Self.logger.error("delegation: signing failed — \(error, privacy: .public)")
            return
        }
        do {
            try await NostrWebSocketEventPublisher().publish(event: signed, relayURL: relayURL)
        } catch {
            Self.logger.error("delegation: publish failed — \(error, privacy: .public)")
            return
        }

        // Record the outgoing turn under the original peer conversation root.
        guard let store = self.store else { return }
        let nonisolatedEncoder = NostrAgentResponder.delegationEventEncoder
        let rawJSON = (try? nonisolatedEncoder.encode(signed))
            .flatMap { String(data: $0, encoding: .utf8) }
        let outgoingTurn = NostrConversationTurn(
            eventID: signed.id,
            direction: .outgoing,
            pubkey: signed.pubkey,
            createdAt: Date(timeIntervalSince1970: TimeInterval(signed.created_at)),
            content: signed.content,
            rawEventJSON: rawJSON
        )
        store.recordNostrTurn(
            rootEventID: peerRootEventID,
            turn: outgoingTurn,
            counterpartyPubkey: peerPubkey
        )
        store.noteNostrActivity(counterpartyPubkey: peerPubkey)
        store.state.nostrRespondedEventIDs.insert(signed.id)
        Self.logger.notice(
            "delegation: published nostrPeer reply \(signed.id.prefix(12), privacy: .public) on root \(peerRootEventID.prefix(12), privacy: .public)"
        )
    }
}
