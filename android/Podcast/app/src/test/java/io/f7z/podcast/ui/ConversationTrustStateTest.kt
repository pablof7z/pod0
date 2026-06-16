package io.f7z.podcast.ui

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Unit tests for [conversationTrustActions] — the pure trust-action state
 * machine that drives the conversation-detail overflow menu.
 *
 * Verifies the defect fix: the menu distinguishes blocked vs explicitly-approved
 * vs follow-only, so it never shows a dead "Remove approval" on a follow-only
 * peer and always offers Unblock recovery for a blocked peer.
 */
class ConversationTrustStateTest {

    @Test
    fun `blocked peer offers Unblock then Approve`() {
        val actions = conversationTrustActions(
            trusted = false,
            peerBlocked = true,
            peerApproved = false,
        )
        assertEquals(
            listOf(ConversationTrustAction.Unblock, ConversationTrustAction.Approve),
            actions,
        )
    }

    @Test
    fun `explicitly approved peer offers RemoveApproval then Block`() {
        val actions = conversationTrustActions(
            trusted = true,
            peerBlocked = false,
            peerApproved = true,
        )
        assertEquals(
            listOf(ConversationTrustAction.RemoveApproval, ConversationTrustAction.Block),
            actions,
        )
    }

    @Test
    fun `follow-only trusted peer offers Block only (no dead Remove approval)`() {
        val actions = conversationTrustActions(
            trusted = true,
            peerBlocked = false,
            peerApproved = false,
        )
        assertEquals(listOf(ConversationTrustAction.Block), actions)
        assert(!actions.contains(ConversationTrustAction.RemoveApproval)) {
            "follow-only peer must NOT offer Remove approval (it would be a no-op)"
        }
    }

    @Test
    fun `untrusted peer offers Approve then Block`() {
        val actions = conversationTrustActions(
            trusted = false,
            peerBlocked = false,
            peerApproved = false,
        )
        assertEquals(
            listOf(ConversationTrustAction.Approve, ConversationTrustAction.Block),
            actions,
        )
    }

    @Test
    fun `blocked peer never offers Remove approval or a second Block`() {
        val actions = conversationTrustActions(
            trusted = false,
            peerBlocked = true,
            peerApproved = false,
        )
        assert(!actions.contains(ConversationTrustAction.RemoveApproval))
        assert(!actions.contains(ConversationTrustAction.Block))
    }

    @Test
    fun `labels are stable user-facing strings`() {
        assertEquals("Approve", ConversationTrustAction.Approve.label())
        assertEquals("Block", ConversationTrustAction.Block.label())
        assertEquals("Remove approval", ConversationTrustAction.RemoveApproval.label())
        assertEquals("Unblock", ConversationTrustAction.Unblock.label())
    }
}
