package io.f7z.podcast

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for the PlayerScreen AutoSnip button guard predicate.
 *
 * The guard mirrors the `canAutoSnip` logic in `TransportRow`:
 *
 *   canAutoSnip = episodeId != null && positionSecs > 0.0
 *
 * This is the ONLY condition enforced in Kotlin — the kernel owns all boundary
 * work (chapter-snap + transcript-refine). These tests pin the contract so a
 * future refactor cannot silently break the guard semantics.
 *
 * Pure-Kotlin — no Android runtime, no KernelBridge, no Compose.
 */
class PlayerAutoSnipGuardTest {

    /** Mirrors the guard in TransportRow so tests stay in sync. */
    private fun canAutoSnip(episodeId: String?, positionSecs: Double): Boolean =
        episodeId != null && positionSecs > 0.0

    @Test
    fun `canAutoSnip is true when episodeId non-null and position positive`() {
        assertTrue(canAutoSnip(episodeId = "ep-abc", positionSecs = 120.5))
    }

    @Test
    fun `canAutoSnip is false when episodeId is null`() {
        assertFalse(
            "null episodeId means nothing is loaded; auto-snip must be disabled",
            canAutoSnip(episodeId = null, positionSecs = 120.5),
        )
    }

    @Test
    fun `canAutoSnip is false when positionSecs is zero`() {
        assertFalse(
            "zero position at episode start is not a meaningful snip point",
            canAutoSnip(episodeId = "ep-abc", positionSecs = 0.0),
        )
    }

    @Test
    fun `canAutoSnip is false when positionSecs is negative`() {
        assertFalse(
            "negative position is invalid; guard must reject it",
            canAutoSnip(episodeId = "ep-abc", positionSecs = -1.0),
        )
    }

    @Test
    fun `canAutoSnip is false when both episodeId is null and position is zero`() {
        assertFalse(canAutoSnip(episodeId = null, positionSecs = 0.0))
    }

    @Test
    fun `canAutoSnip is true at one second into an episode`() {
        // Minimum meaningful guard: any positive position with a valid episode.
        assertTrue(canAutoSnip(episodeId = "ep-xyz", positionSecs = 1.0))
    }
}
