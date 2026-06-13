import Foundation

// MARK: - KernelModel social approval actions
//
// Thin dispatch shims for the four kernel-owned trust actions. Each call
// serialises to a `podcast.social` host-op that mutates the kernel's
// `ApprovedPeerStore`, persists it to `approved-peers.json`, and bumps the
// `podcast.social` domain rev so the next projection push reflects the change.
//
// The trust predicate that actually gates auto-replies and filters the
// conversation view lives entirely in Rust:
//
//   trust(pubkey) = (followed || approved) && !blocked
//
// Swift NEVER re-implements that logic; it only fires these one-way commands
// and reads the resulting `trusted` flag from the next `NostrConversationDTO`
// or `AgentNoteSummary` emitted by the kernel.

extension KernelModel {

    /// Approve `hex` as a trusted sender. Clears any existing block for that
    /// pubkey (kernel semantics: approve and block are mutually exclusive).
    @discardableResult
    func approvePeer(hex: String) -> DispatchResult {
        dispatch(namespace: DomainSchema.social,
                 body: ["op": "approve_peer", "pubkey_hex": hex])
    }

    /// Block `hex`. Clears any existing approval for that pubkey (kernel
    /// semantics: block is an absolute override of both follow and approval).
    @discardableResult
    func blockPeer(hex: String) -> DispatchResult {
        dispatch(namespace: DomainSchema.social,
                 body: ["op": "block_peer", "pubkey_hex": hex])
    }

    /// Remove an explicit approval for `hex`. The peer reverts to untrusted
    /// unless they are in the NIP-02 follow list.
    @discardableResult
    func removePeerApproval(hex: String) -> DispatchResult {
        dispatch(namespace: DomainSchema.social,
                 body: ["op": "remove_approval", "pubkey_hex": hex])
    }

    /// Lift a block for `hex`. The peer reverts to untrusted-but-not-blocked
    /// (their trust state then depends on follow list and approval).
    @discardableResult
    func removePeerBlock(hex: String) -> DispatchResult {
        dispatch(namespace: DomainSchema.social,
                 body: ["op": "remove_block", "pubkey_hex": hex])
    }
}
