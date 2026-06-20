package io.f7z.podcast

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for [DispatchResult.parseEnvelope].
 *
 * Verifies the Kotlin envelope parser matches the Swift
 * `DispatchResult.parse(envelope:)` in `KernelBridge+Decode.swift` exactly.
 *
 * Kernel wire contract (`ActionRegistry::start`):
 *   Accepted : `{"correlation_id":"<32-hex>"}`
 *   Rejected : `{"error":"<human-readable reason>"}`
 *   FFI fail : null return from `nmp_app_dispatch_action`
 *
 * Regression guard for the two Android bugs fixed in this PR:
 *   Bug 1 — EditProfileScreen used `contains("\"ok\":false")`, which never
 *            matched a real rejection envelope → error responses silently
 *            fell to the success branch ("Profile update sent.").
 *   Bug 2 — IdentityActions.publishProfile cached profile fields before
 *            checking the result → a rejected dispatch still wrote the cache.
 */
class KernelDispatchResultTest {

    // ── Accepted envelope ─────────────────────────────────────────────────────

    @Test
    fun `parseEnvelope accepted envelope returns Accepted`() {
        val raw = """{"correlation_id":"abc123def456"}"""
        val result = DispatchResult.parseEnvelope(raw)
        assertTrue("Expected Accepted, got $result", result is DispatchResult.Accepted)
        assertEquals("abc123def456", (result as DispatchResult.Accepted).correlationId)
    }

    @Test
    fun `parseEnvelope accepted envelope with extra fields still Accepted`() {
        // Kernel may add forward-compatible fields — parser must not reject them.
        val raw = """{"correlation_id":"id42","ts":1718000000}"""
        val result = DispatchResult.parseEnvelope(raw)
        assertTrue(result is DispatchResult.Accepted)
        assertEquals("id42", (result as DispatchResult.Accepted).correlationId)
    }

    // ── Rejected envelope ─────────────────────────────────────────────────────

    @Test
    fun `parseEnvelope error envelope returns Failure with kernel message`() {
        val raw = """{"error":"no active account"}"""
        val result = DispatchResult.parseEnvelope(raw)
        assertTrue("Expected Failure, got $result", result is DispatchResult.Failure)
        assertEquals("no active account", (result as DispatchResult.Failure).message)
    }

    @Test
    fun `parseEnvelope error envelope does NOT trigger false success`() {
        // Regression: old EditProfileScreen code checked contains("\"ok\":false")
        // but then fell to the `else` branch for real {"error":"..."} responses.
        val raw = """{"error":"publish rejected: relay unavailable"}"""
        val result = DispatchResult.parseEnvelope(raw)
        assertTrue(
            "A rejection envelope must NEVER be treated as Accepted",
            result is DispatchResult.Failure,
        )
    }

    // ── null / FFI failure ────────────────────────────────────────────────────

    @Test
    fun `parseEnvelope null returns Failure`() {
        val result = DispatchResult.parseEnvelope(null)
        assertTrue("null FFI return must be Failure", result is DispatchResult.Failure)
    }

    // ── Old wrong format must not be silently accepted ─────────────────────────

    @Test
    fun `parseEnvelope ok-false old format is Failure`() {
        // Regression guard: the old Android detection used contains("\"ok\":false").
        // The kernel never emits {"ok":...}. If it somehow did, it must not silently
        // succeed because it has no correlation_id.
        val raw = """{"ok":false,"error":"some reason"}"""
        val result = DispatchResult.parseEnvelope(raw)
        assertTrue(
            "ok:false is not the real kernel envelope — must be Failure",
            result is DispatchResult.Failure,
        )
    }

    @Test
    fun `parseEnvelope ok-true old format is also Failure`() {
        // {"ok":true} is NOT the real accepted envelope.
        // The real accepted envelope is {"correlation_id":"..."}.
        // An {"ok":true} response has no correlation_id → Failure.
        val raw = """{"ok":true}"""
        val result = DispatchResult.parseEnvelope(raw)
        assertTrue(
            "ok:true is not the real accepted envelope — must be Failure without correlation_id",
            result is DispatchResult.Failure,
        )
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    @Test
    fun `parseEnvelope empty correlation_id is Failure`() {
        // An empty string correlation_id is not a valid accept.
        val raw = """{"correlation_id":""}"""
        val result = DispatchResult.parseEnvelope(raw)
        assertTrue(
            "Empty correlation_id must not be treated as Accepted",
            result is DispatchResult.Failure,
        )
    }

    @Test
    fun `parseEnvelope malformed JSON is Failure`() {
        val result = DispatchResult.parseEnvelope("not json at all")
        assertTrue(result is DispatchResult.Failure)
    }

    @Test
    fun `parseEnvelope empty JSON object is Failure`() {
        val result = DispatchResult.parseEnvelope("{}")
        assertTrue(result is DispatchResult.Failure)
    }
}
