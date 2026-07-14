import Foundation

// MARK: - Nostr Access Control

extension AppStateStore {

    func allowNostrPubkey(_ pubkeyHex: String) {
        state.nostrAllowedPubkeys.insert(pubkeyHex)
        state.nostrBlockedPubkeys.remove(pubkeyHex)
        state.nostrPendingApprovals.removeAll { $0.pubkeyHex == pubkeyHex }
    }

    func blockNostrPubkey(_ pubkeyHex: String) {
        state.nostrBlockedPubkeys.insert(pubkeyHex)
        state.nostrAllowedPubkeys.remove(pubkeyHex)
        state.nostrPendingApprovals.removeAll { $0.pubkeyHex == pubkeyHex }
    }

    func removeFromNostrAllowlist(_ pubkeyHex: String) {
        state.nostrAllowedPubkeys.remove(pubkeyHex)
    }

    func removeFromNostrBlocklist(_ pubkeyHex: String) {
        state.nostrBlockedPubkeys.remove(pubkeyHex)
    }

    func addNostrPendingApproval(_ approval: NostrPendingApproval) {
        guard !state.nostrAllowedPubkeys.contains(approval.pubkeyHex),
              !state.nostrBlockedPubkeys.contains(approval.pubkeyHex),
              !state.nostrPendingApprovals.contains(where: { $0.pubkeyHex == approval.pubkeyHex })
        else { return }
        state.nostrPendingApprovals.append(approval)
    }

    /// Fills in display name / about / picture for an existing pending
    /// approval when a kind:0 profile arrives after the inbound has been
    /// queued. No-op when the pubkey is not pending.
    func enrichNostrPendingApproval(pubkeyHex: String, from profile: NostrProfileMetadata) {
        guard let idx = state.nostrPendingApprovals.firstIndex(where: { $0.pubkeyHex == pubkeyHex })
        else { return }
        var approval = state.nostrPendingApprovals[idx]
        approval.displayName = profile.bestLabel ?? approval.displayName
        approval.about = profile.about ?? approval.about
        approval.pictureURL = profile.picture ?? approval.pictureURL
        state.nostrPendingApprovals[idx] = approval
    }

    func dismissNostrPendingApproval(_ id: UUID) {
        state.nostrPendingApprovals.removeAll { $0.id == id }
    }

    var pendingNostrApprovals: [NostrPendingApproval] {
        state.nostrPendingApprovals
    }

    // MARK: - Nostr Conversations

    /// Appends `turn` to the conversation with `rootEventID`, creating the
    /// record on first sight. `counterparty` is required for the create
    /// path when the turn is outgoing (the agent's own pubkey is not the
    /// counterparty); for incoming turns `turn.pubkey` is used.
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
