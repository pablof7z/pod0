package io.f7z.podcast

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Unit tests for [SocialActions] wire-payload builders.
 *
 * Asserts the exact JSON shapes expected by the Rust kernel's
 * `apps/nmp-app-podcast/src/ffi/actions/social_module.rs::SocialAction`:
 *
 * ```rust
 * #[serde(tag = "op", rename_all = "snake_case")]
 * pub enum SocialAction {
 *     ApprovePeer    { pubkey_hex: String },
 *     BlockPeer      { pubkey_hex: String },
 *     RemoveApproval { pubkey_hex: String },
 *     RemoveBlock    { pubkey_hex: String },
 * }
 * ```
 *
 * Critical contract: `pubkey_hex` MUST be spelled exactly in snake_case —
 * Android has NO automatic camelCase→snake_case conversion; wrong casing
 * silently drops the field and the kernel never sees the pubkey.
 */
class SocialActionsTest {

    private val json = Json

    private fun parse(payload: String): JsonObject =
        json.decodeFromString(JsonObject.serializer(), payload)

    // ── NAMESPACE constant ────────────────────────────────────────────────────

    @Test
    fun `NAMESPACE matches Rust SocialActionModule NAMESPACE`() {
        assertEquals(
            "NAMESPACE must match Rust SocialActionModule::NAMESPACE = \"podcast.social\"",
            "podcast.social",
            SocialActions.NAMESPACE,
        )
    }

    // ── approve_peer ──────────────────────────────────────────────────────────

    @Test
    fun `buildApprovePeerPayload op is approve_peer`() {
        val obj = parse(SocialActions.buildApprovePeerPayload("aabbcc"))
        assertEquals(
            "op must be 'approve_peer' (Rust SocialAction::ApprovePeer rename_all=snake_case)",
            "approve_peer",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildApprovePeerPayload encodes pubkey_hex in snake_case`() {
        val obj = parse(SocialActions.buildApprovePeerPayload("deadbeef"))
        assertEquals(
            "field must be 'pubkey_hex' not 'pubkeyHex' — Android has no auto snake_case",
            "deadbeef",
            obj["pubkey_hex"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildApprovePeerPayload shape is exactly op + pubkey_hex`() {
        val obj = parse(SocialActions.buildApprovePeerPayload("aabbcc"))
        assertEquals("approve_peer payload must have exactly 2 fields", 2, obj.size)
    }

    @Test
    fun `buildApprovePeerPayload full wire shape`() {
        val obj = parse(SocialActions.buildApprovePeerPayload("abc123"))
        assertEquals("approve_peer", obj["op"]?.jsonPrimitive?.content)
        assertEquals("abc123", obj["pubkey_hex"]?.jsonPrimitive?.content)
    }

    // ── block_peer ────────────────────────────────────────────────────────────

    @Test
    fun `buildBlockPeerPayload op is block_peer`() {
        val obj = parse(SocialActions.buildBlockPeerPayload("aabbcc"))
        assertEquals(
            "op must be 'block_peer' (Rust SocialAction::BlockPeer rename_all=snake_case)",
            "block_peer",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildBlockPeerPayload encodes pubkey_hex in snake_case`() {
        val obj = parse(SocialActions.buildBlockPeerPayload("cafebabe"))
        assertEquals(
            "field must be 'pubkey_hex' not 'pubkeyHex'",
            "cafebabe",
            obj["pubkey_hex"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildBlockPeerPayload shape is exactly op + pubkey_hex`() {
        val obj = parse(SocialActions.buildBlockPeerPayload("aabbcc"))
        assertEquals("block_peer payload must have exactly 2 fields", 2, obj.size)
    }

    @Test
    fun `buildBlockPeerPayload full wire shape`() {
        val obj = parse(SocialActions.buildBlockPeerPayload("def456"))
        assertEquals("block_peer", obj["op"]?.jsonPrimitive?.content)
        assertEquals("def456", obj["pubkey_hex"]?.jsonPrimitive?.content)
    }

    // ── remove_approval ───────────────────────────────────────────────────────

    @Test
    fun `buildRemoveApprovalPayload op is remove_approval`() {
        val obj = parse(SocialActions.buildRemoveApprovalPayload("aabbcc"))
        assertEquals(
            "op must be 'remove_approval' (Rust SocialAction::RemoveApproval rename_all=snake_case)",
            "remove_approval",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildRemoveApprovalPayload encodes pubkey_hex in snake_case`() {
        val obj = parse(SocialActions.buildRemoveApprovalPayload("112233"))
        assertEquals("112233", obj["pubkey_hex"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildRemoveApprovalPayload shape is exactly op + pubkey_hex`() {
        val obj = parse(SocialActions.buildRemoveApprovalPayload("aabbcc"))
        assertEquals("remove_approval payload must have exactly 2 fields", 2, obj.size)
    }

    // ── remove_block ──────────────────────────────────────────────────────────

    @Test
    fun `buildRemoveBlockPayload op is remove_block`() {
        val obj = parse(SocialActions.buildRemoveBlockPayload("aabbcc"))
        assertEquals(
            "op must be 'remove_block' (Rust SocialAction::RemoveBlock rename_all=snake_case)",
            "remove_block",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildRemoveBlockPayload encodes pubkey_hex in snake_case`() {
        val obj = parse(SocialActions.buildRemoveBlockPayload("ffaabb"))
        assertEquals("ffaabb", obj["pubkey_hex"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildRemoveBlockPayload shape is exactly op + pubkey_hex`() {
        val obj = parse(SocialActions.buildRemoveBlockPayload("aabbcc"))
        assertEquals("remove_block payload must have exactly 2 fields", 2, obj.size)
    }

    // ── all ops are distinct ──────────────────────────────────────────────────

    @Test
    fun `all four op strings are distinct`() {
        val hex = "aabbcc"
        val ops = listOf(
            parse(SocialActions.buildApprovePeerPayload(hex))["op"]?.jsonPrimitive?.content,
            parse(SocialActions.buildBlockPeerPayload(hex))["op"]?.jsonPrimitive?.content,
            parse(SocialActions.buildRemoveApprovalPayload(hex))["op"]?.jsonPrimitive?.content,
            parse(SocialActions.buildRemoveBlockPayload(hex))["op"]?.jsonPrimitive?.content,
        )
        assertEquals("All four op strings must be distinct", 4, ops.distinct().size)
    }
}
