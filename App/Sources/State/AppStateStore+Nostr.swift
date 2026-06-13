import Foundation

// MARK: - Nostr Access Control
//
// The kernel (Rust, `ApprovedPeerStore`) is authoritative for approved and
// blocked pubkeys.  Swift dispatches one-way mutations via `KernelModel` and
// keeps `nostrAllowedPubkeys` / `nostrBlockedPubkeys` on `AppState` as an
// optimistic display mirror; the authoritative state re-arrives on the next
// `podcast.social` domain push as `trusted` flags on each
// `NostrConversationDTO`.
//
// The old `nostrPendingApprovals` / `NostrPendingApproval` scaffolding has
// been deleted.  Unknown senders are simply untrusted (kernel-gated) until the
// user explicitly approves them via `AgentAccessControlView`.

extension AppStateStore {

    // MARK: - Kernel-routed mutations

    func allowNostrPubkey(_ pubkeyHex: String) {
        // Optimistic mirror.
        state.nostrAllowedPubkeys.insert(pubkeyHex)
        state.nostrBlockedPubkeys.remove(pubkeyHex)
        // Durable kernel write.
        kernel?.approvePeer(hex: pubkeyHex)
    }

    func blockNostrPubkey(_ pubkeyHex: String) {
        // Optimistic mirror.
        state.nostrBlockedPubkeys.insert(pubkeyHex)
        state.nostrAllowedPubkeys.remove(pubkeyHex)
        // Durable kernel write.
        kernel?.blockPeer(hex: pubkeyHex)
    }

    func removeFromNostrAllowlist(_ pubkeyHex: String) {
        state.nostrAllowedPubkeys.remove(pubkeyHex)
        kernel?.removePeerApproval(hex: pubkeyHex)
    }

    func removeFromNostrBlocklist(_ pubkeyHex: String) {
        state.nostrBlockedPubkeys.remove(pubkeyHex)
        kernel?.removePeerBlock(hex: pubkeyHex)
    }

    // MARK: - Nostr Conversations

    /// Appends `turn` to the conversation with `rootEventID`, creating the
    /// record on first sight. `counterparty` is required for the create
    /// path when the turn is outgoing (the agent's own pubkey is not the
    /// counterparty); for incoming turns `turn.pubkey` is used.
    ///
    /// LEGACY: The kernel (`podcast.social` domain, `nostr_conversations_snapshot`)
    /// is now AUTHORITATIVE for the conversation projection and replaces this
    /// slice on each push. This method remains as a local-echo / optimistic-update
    /// path so outgoing turns appear immediately before the next kernel push.
    func recordNostrTurn(
        rootEventID: String,
        turn: NostrConversationTurn,
        counterpartyPubkey: String? = nil
    ) {
        if let idx = state.nostrConversations.firstIndex(where: { $0.rootEventID == rootEventID }) {
            if !state.nostrConversations[idx].turns.contains(where: { $0.eventID == turn.eventID }) {
                state.nostrConversations[idx].turns.append(turn)
            }
            state.nostrConversations[idx].lastTouched = Date()
        } else {
            let resolved = counterpartyPubkey ?? turn.pubkey
            state.nostrConversations.append(
                NostrConversationRecord(
                    rootEventID: rootEventID,
                    counterpartyPubkey: resolved,
                    firstSeen: Date(),
                    lastTouched: Date(),
                    turns: [turn]
                )
            )
        }
    }

    /// Surfaces (or refreshes) the floating "Talking to X" capsule for the
    /// configured `nostrActivityIndicatorDuration`. Called by
    /// `NostrAgentResponder` after every incoming or outgoing turn so a
    /// back-and-forth keeps the capsule continuously on screen instead of
    /// flickering between turns.
    func noteNostrActivity(counterpartyPubkey: String) {
        activeNostrCounterparty = counterpartyPubkey
        nostrActivityDismissTask?.cancel()
        nostrActivityDismissTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(AppStateStore.nostrActivityIndicatorDuration))
            guard !Task.isCancelled else { return }
            self?.activeNostrCounterparty = nil
        }
    }

    /// Inserts or upgrades a cached profile. Older `kind:0` events are
    /// ignored so a relay re-delivery never downgrades a fresher profile.
    func setNostrProfile(_ profile: NostrProfileMetadata) {
        if let existing = state.nostrProfileCache[profile.pubkey],
           existing.fetchedFromCreatedAt >= profile.fetchedFromCreatedAt {
            return
        }
        state.nostrProfileCache[profile.pubkey] = profile
    }
}
