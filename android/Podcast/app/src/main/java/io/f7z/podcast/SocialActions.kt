package io.f7z.podcast

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * Canonical wire contract for the `podcast.social` kernel action namespace.
 *
 * Wire shapes verified against
 * `apps/nmp-app-podcast/src/ffi/actions/social_module.rs`
 * (`SocialActionModule::NAMESPACE = "podcast.social"`):
 *
 *  `approve_peer`    — `{"op":"approve_peer","pubkey_hex":"…"}`
 *                      Clears any existing block (approve and block are mutually exclusive).
 *  `block_peer`      — `{"op":"block_peer","pubkey_hex":"…"}`
 *                      Absolute override of follow + approval. Clears any existing approval.
 *  `remove_approval` — `{"op":"remove_approval","pubkey_hex":"…"}`
 *                      Reverts to follow-only trust (no explicit approval remains).
 *  `remove_block`    — `{"op":"remove_block","pubkey_hex":"…"}`
 *                      Lifts the block; trust then depends on follow + approval state.
 *
 * The Rust enum uses `#[serde(tag = "op", rename_all = "snake_case")]`, so op
 * values are the snake_case variant names: `ApprovePeer` → `"approve_peer"`, etc.
 *
 * **`pubkey_hex` must be spelled exactly** — Android has NO automatic
 * snake_case conversion, unlike the iOS bridge. Wrong casing silently drops
 * the field (past trap documented in ActionDispatcher.kt).
 *
 * Payload builders are pure functions (no [KernelBridge] dependency) so they
 * can be tested without the native library loaded — same pattern as [ClipActions].
 */
object SocialActions {

    /**
     * Action namespace.
     * Source of truth: `SocialActionModule::NAMESPACE` in
     * `apps/nmp-app-podcast/src/ffi/actions/social_module.rs`.
     */
    const val NAMESPACE = "podcast.social"

    private val json = Json

    // ── Public dispatch helpers ──────────────────────────────────────────────

    /**
     * Dispatch `podcast.social` `approve_peer` to the kernel.
     *
     * Clears any existing block for [pubkeyHex] (approve and block are
     * mutually exclusive in the kernel's `ApprovedPeerStore`). The kernel
     * persists the store and bumps the `podcast.social` domain rev so the
     * next projection push reflects the updated `trusted` verdict.
     */
    fun approvePeer(bridge: KernelBridge, pubkeyHex: String): String? =
        bridge.dispatchAction(NAMESPACE, buildApprovePeerPayload(pubkeyHex))

    /**
     * Dispatch `podcast.social` `block_peer` to the kernel.
     *
     * Block is an absolute override of follow status and explicit approval.
     * Clears any existing approval for [pubkeyHex].
     */
    fun blockPeer(bridge: KernelBridge, pubkeyHex: String): String? =
        bridge.dispatchAction(NAMESPACE, buildBlockPeerPayload(pubkeyHex))

    /**
     * Dispatch `podcast.social` `remove_approval` to the kernel.
     *
     * Reverts [pubkeyHex] to follow-only trust. If the peer is not in the
     * NIP-02 follow list, they revert to untrusted.
     */
    fun removeApproval(bridge: KernelBridge, pubkeyHex: String): String? =
        bridge.dispatchAction(NAMESPACE, buildRemoveApprovalPayload(pubkeyHex))

    /**
     * Dispatch `podcast.social` `remove_block` to the kernel.
     *
     * Lifts the block for [pubkeyHex]. Trust is then determined by whether
     * the peer is followed or explicitly approved.
     */
    fun removeBlock(bridge: KernelBridge, pubkeyHex: String): String? =
        bridge.dispatchAction(NAMESPACE, buildRemoveBlockPayload(pubkeyHex))

    // ── Pure payload builders (testable without bridge) ──────────────────────

    /**
     * Build the `approve_peer` wire payload.
     *
     * Rust contract (`SocialAction::ApprovePeer`):
     * ```json
     * {"op":"approve_peer","pubkey_hex":"<hex>"}
     * ```
     */
    fun buildApprovePeerPayload(pubkeyHex: String): String =
        buildPeerPayload("approve_peer", pubkeyHex)

    /**
     * Build the `block_peer` wire payload.
     *
     * Rust contract (`SocialAction::BlockPeer`):
     * ```json
     * {"op":"block_peer","pubkey_hex":"<hex>"}
     * ```
     */
    fun buildBlockPeerPayload(pubkeyHex: String): String =
        buildPeerPayload("block_peer", pubkeyHex)

    /**
     * Build the `remove_approval` wire payload.
     *
     * Rust contract (`SocialAction::RemoveApproval`):
     * ```json
     * {"op":"remove_approval","pubkey_hex":"<hex>"}
     * ```
     */
    fun buildRemoveApprovalPayload(pubkeyHex: String): String =
        buildPeerPayload("remove_approval", pubkeyHex)

    /**
     * Build the `remove_block` wire payload.
     *
     * Rust contract (`SocialAction::RemoveBlock`):
     * ```json
     * {"op":"remove_block","pubkey_hex":"<hex>"}
     * ```
     */
    fun buildRemoveBlockPayload(pubkeyHex: String): String =
        buildPeerPayload("remove_block", pubkeyHex)

    // ── Internal ─────────────────────────────────────────────────────────────

    private fun buildPeerPayload(op: String, pubkeyHex: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "op"         to JsonPrimitive(op),
                    "pubkey_hex" to JsonPrimitive(pubkeyHex),
                ),
            ),
        )
}
