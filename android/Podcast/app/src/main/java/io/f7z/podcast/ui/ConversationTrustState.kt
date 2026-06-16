package io.f7z.podcast.ui

/**
 * The trust actions offered for a conversation counterparty, derived from the
 * kernel's explicit per-peer flags.
 *
 * Each action maps 1:1 to a `podcast.social` kernel op:
 *  - [Approve]        → `approve_peer`    (clears any block)
 *  - [Block]          → `block_peer`      (absolute override; confirm first)
 *  - [RemoveApproval] → `remove_approval` (revert explicit approval → follow-only)
 *  - [Unblock]        → `remove_block`    (lift block → follow-only/untrusted)
 */
enum class ConversationTrustAction {
    Approve,
    Block,
    RemoveApproval,
    Unblock,
}

/**
 * Pure state machine mapping a conversation's kernel trust flags to the set of
 * actions the detail-screen menu should surface. No Compose/Android deps so it
 * is unit-testable on the JVM.
 *
 * The three kernel flags are NOT independent — the kernel guarantees:
 *  - `peerBlocked` ⟹ `!trusted` (block is an absolute override)
 *  - `peerApproved` ⟹ `trusted` (explicit approval grants trust)
 *  - `trusted && !peerApproved` ⟹ follow-only trust (NIP-02 follow, no explicit approval)
 *
 * State → actions:
 *  | State                                  | Actions                       |
 *  |----------------------------------------|-------------------------------|
 *  | blocked (peerBlocked)                  | Unblock, Approve              |
 *  | explicitly approved (peerApproved)     | RemoveApproval, Block         |
 *  | follow-only (trusted && !peerApproved) | Block                         |
 *  | untrusted (!trusted && !peerBlocked)   | Approve, Block                |
 *
 * Order matters — [peerBlocked] is checked first because it is the absolute
 * override; a blocked peer offers Unblock as the primary recovery (and Approve
 * for the user who wants to trust them outright, since approve clears the block).
 * Follow-only deliberately omits "Remove approval": there is no explicit
 * approval to remove, so that action would be a no-op dead-end.
 */
fun conversationTrustActions(
    trusted: Boolean,
    peerBlocked: Boolean,
    peerApproved: Boolean,
): List<ConversationTrustAction> = when {
    peerBlocked -> listOf(ConversationTrustAction.Unblock, ConversationTrustAction.Approve)
    peerApproved -> listOf(ConversationTrustAction.RemoveApproval, ConversationTrustAction.Block)
    trusted -> listOf(ConversationTrustAction.Block)
    else -> listOf(ConversationTrustAction.Approve, ConversationTrustAction.Block)
}
