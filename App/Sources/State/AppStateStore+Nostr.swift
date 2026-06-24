import Foundation

// MARK: - Nostr Access Control
//
// The kernel (Rust, `ApprovedPeerStore`) is authoritative for approved and
// blocked pubkeys.  Swift dispatches one-way mutations via `KernelModel`; the
// authoritative state is read from `podcast.social` domain push via
// `podcastSnapshot?.social` (approvedPubkeys / blockedPubkeys).
//
// The old `nostrPendingApprovals` / `NostrPendingApproval` scaffolding has
// been deleted.  Unknown senders are simply untrusted (kernel-gated) until the
// user explicitly approves them via `AgentAccessControlView`.

extension AppStateStore {

    // MARK: - Kernel-routed mutations

    func allowNostrPubkey(_ pubkeyHex: String) {
        kernel?.approvePeer(hex: pubkeyHex)
    }

    func blockNostrPubkey(_ pubkeyHex: String) {
        kernel?.blockPeer(hex: pubkeyHex)
    }

    func removeFromNostrAllowlist(_ pubkeyHex: String) {
        kernel?.removePeerApproval(hex: pubkeyHex)
    }

    func removeFromNostrBlocklist(_ pubkeyHex: String) {
        kernel?.removePeerBlock(hex: pubkeyHex)
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
