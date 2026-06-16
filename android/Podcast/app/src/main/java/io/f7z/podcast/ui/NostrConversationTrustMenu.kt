package io.f7z.podcast.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.SocialActions

/**
 * Overflow menu for conversation-level trust actions, placed in the
 * [NostrConversationDetailScreen] TopAppBar.
 *
 * The action set is driven by [conversationTrustActions] from the kernel's
 * explicit per-peer flags ([trusted], [peerBlocked], [peerApproved]) — so the
 * menu correctly distinguishes blocked vs explicitly-approved vs follow-only,
 * which the composed `trusted` bool alone cannot. Each action maps 1:1 to a
 * `podcast.social` kernel op via [SocialActions].
 *
 * **REACTIVE contract**: this composable NEVER mutates trust state locally. It
 * dispatches commands and lets the kernel push updated
 * `NostrConversationDto.{trusted,peerBlocked,peerApproved}` on the next snapshot
 * frame, which flows through the reactive push seam and recomposes this menu.
 * No polling, no optimistic state (android_reactive_path_and_datadir).
 *
 * Guards against a blank counterparty hex (filtered conversation / tombstone):
 * the menu is not rendered, so no `pubkey_hex`-less dispatch can reach the kernel.
 */
@Composable
fun NostrConversationTrustMenu(
    counterpartyHex: String,
    trusted: Boolean,
    peerBlocked: Boolean,
    peerApproved: Boolean,
    bridge: KernelBridge,
) {
    // Do not surface trust controls without a valid peer to act on.
    if (counterpartyHex.isBlank()) return

    var showMenu by remember { mutableStateOf(false) }
    var showBlockConfirm by remember { mutableStateOf(false) }

    val actions = conversationTrustActions(
        trusted = trusted,
        peerBlocked = peerBlocked,
        peerApproved = peerApproved,
    )

    IconButton(onClick = { showMenu = true }) {
        Icon(
            imageVector = Icons.Default.MoreVert,
            contentDescription = "Trust actions",
        )
    }

    DropdownMenu(
        expanded = showMenu,
        onDismissRequest = { showMenu = false },
    ) {
        actions.forEach { action ->
            DropdownMenuItem(
                text = { Text(action.label()) },
                onClick = {
                    showMenu = false
                    when (action) {
                        // Block is destructive + absolute → confirm first.
                        ConversationTrustAction.Block -> showBlockConfirm = true
                        ConversationTrustAction.Approve ->
                            SocialActions.approvePeer(bridge, counterpartyHex)
                        ConversationTrustAction.RemoveApproval ->
                            SocialActions.removeApproval(bridge, counterpartyHex)
                        ConversationTrustAction.Unblock ->
                            SocialActions.removeBlock(bridge, counterpartyHex)
                    }
                },
            )
        }
    }

    if (showBlockConfirm) {
        AlertDialog(
            onDismissRequest = { showBlockConfirm = false },
            title = { Text("Block this peer?") },
            text = {
                Text(
                    "This peer will be blocked from contacting your agent. " +
                        "Block overrides follow status and any approval. " +
                        "You can unblock this person later from this menu.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        SocialActions.blockPeer(bridge, counterpartyHex)
                        showBlockConfirm = false
                    },
                ) {
                    Text("Block")
                }
            },
            dismissButton = {
                TextButton(onClick = { showBlockConfirm = false }) {
                    Text("Cancel")
                }
            },
        )
    }
}

/** User-facing label for each trust action. */
internal fun ConversationTrustAction.label(): String = when (this) {
    ConversationTrustAction.Approve -> "Approve"
    ConversationTrustAction.Block -> "Block"
    ConversationTrustAction.RemoveApproval -> "Remove approval"
    ConversationTrustAction.Unblock -> "Unblock"
}
