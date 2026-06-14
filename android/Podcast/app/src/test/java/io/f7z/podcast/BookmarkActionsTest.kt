package io.f7z.podcast

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for [BookmarkActions] wire-payload builders.
 *
 * Asserts the exact JSON shapes expected by
 * `apps/nmp-app-podcast/src/ffi/actions/podcast_module.rs::PodcastAction::StarEpisode`:
 *
 * ```rust
 * #[serde(tag = "op", rename_all = "snake_case")]
 * pub enum PodcastAction {
 *     StarEpisode {
 *         episode_id: String,
 *         #[serde(default, skip_serializing_if = "Option::is_none")]
 *         starred: Option<bool>,
 *     },
 *     // ...
 * }
 * ```
 *
 * Namespace constant must match `PodcastActionModule::NAMESPACE = "podcast"`.
 *
 * All tests are pure-Kotlin (no Android runtime, no KernelBridge).
 */
class BookmarkActionsTest {

    private val json = Json { ignoreUnknownKeys = true }

    // ── buildTogglePayload ────────────────────────────────────────────────────

    @Test
    fun `buildTogglePayload op field is 'star_episode'`() {
        val payload = BookmarkActions.buildTogglePayload("ep-uuid-1")
        val obj = json.parseToJsonElement(payload).jsonObject
        assertEquals(
            "op must be 'star_episode' (Rust PodcastAction::StarEpisode rename_all=snake_case)",
            "star_episode",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildTogglePayload encodes episode_id as snake_case`() {
        val payload = BookmarkActions.buildTogglePayload("ep-abc-123")
        val obj = json.parseToJsonElement(payload).jsonObject
        assertEquals(
            "episode_id must be snake_case (matches Rust StarEpisode field name)",
            "ep-abc-123",
            obj["episode_id"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildTogglePayload omits starred field (kernel flips current value)`() {
        val payload = BookmarkActions.buildTogglePayload("ep-uuid-1")
        val obj = json.parseToJsonElement(payload).jsonObject
        assertNull(
            "starred must be absent from toggle payload — Rust skip_serializing_if=Option::is_none",
            obj["starred"],
        )
    }

    @Test
    fun `buildTogglePayload produces valid JSON object`() {
        val payload = BookmarkActions.buildTogglePayload("ep-uuid-42")
        val parsed = json.parseToJsonElement(payload)
        assertTrue("toggle payload must decode as a JSON object", parsed is JsonObject)
    }

    // ── buildSetStarPayload ───────────────────────────────────────────────────

    @Test
    fun `buildSetStarPayload op field is 'star_episode'`() {
        val payload = BookmarkActions.buildSetStarPayload("ep-uuid-1", starred = true)
        val obj = json.parseToJsonElement(payload).jsonObject
        assertEquals(
            "op must be 'star_episode'",
            "star_episode",
            obj["op"]?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `buildSetStarPayload encodes episode_id`() {
        val payload = BookmarkActions.buildSetStarPayload("ep-xyz", starred = false)
        val obj = json.parseToJsonElement(payload).jsonObject
        assertEquals("ep-xyz", obj["episode_id"]?.jsonPrimitive?.content)
    }

    @Test
    fun `buildSetStarPayload includes starred=true when set to true`() {
        val payload = BookmarkActions.buildSetStarPayload("ep-1", starred = true)
        val obj = json.parseToJsonElement(payload).jsonObject
        assertTrue(
            "starred must be present and true when explicitly set to true",
            obj["starred"]?.jsonPrimitive?.boolean == true,
        )
    }

    @Test
    fun `buildSetStarPayload includes starred=false when set to false`() {
        val payload = BookmarkActions.buildSetStarPayload("ep-1", starred = false)
        val obj = json.parseToJsonElement(payload).jsonObject
        assertTrue(
            "starred must be present and false when explicitly set to false (unstar)",
            obj["starred"]?.jsonPrimitive?.boolean == false,
        )
    }

    @Test
    fun `buildSetStarPayload produces valid JSON object`() {
        val payload = BookmarkActions.buildSetStarPayload("ep-uuid-99", starred = true)
        val parsed = json.parseToJsonElement(payload)
        assertTrue("set-star payload must decode as a JSON object", parsed is JsonObject)
    }

    // ── isStarred filter logic ────────────────────────────────────────────────

    @Test
    fun `starred episodes filter returns only starred episodes`() {
        val starred1 = makeEpisode(id = "ep-1", starred = true)
        val starred2 = makeEpisode(id = "ep-2", starred = true)
        val notStarred = makeEpisode(id = "ep-3", starred = false)

        val all = listOf(starred1, starred2, notStarred)
        val filtered = all.filter { it.starred }

        assertEquals(
            "filter must return exactly 2 starred episodes",
            2,
            filtered.size,
        )
        assertTrue("ep-1 must be in filtered list", filtered.any { it.id == "ep-1" })
        assertTrue("ep-2 must be in filtered list", filtered.any { it.id == "ep-2" })
        assertTrue("ep-3 must NOT be in filtered list", filtered.none { it.id == "ep-3" })
    }

    @Test
    fun `starred episodes filter returns empty list when none starred`() {
        val all = listOf(
            makeEpisode("ep-1", starred = false),
            makeEpisode("ep-2", starred = false),
        )
        val filtered = all.filter { it.starred }
        assertTrue("filter must return empty list when no starred episodes", filtered.isEmpty())
    }

    @Test
    fun `starred episodes filter returns all when all starred`() {
        val all = listOf(
            makeEpisode("ep-1", starred = true),
            makeEpisode("ep-2", starred = true),
        )
        val filtered = all.filter { it.starred }
        assertEquals("filter must return all episodes when all starred", 2, filtered.size)
    }

    @Test
    fun `starred episodes sort newest-first by publishedAt`() {
        val older = makeEpisode("ep-old", starred = true, publishedAt = 1000L)
        val newer = makeEpisode("ep-new", starred = true, publishedAt = 2000L)
        val mid = makeEpisode("ep-mid", starred = true, publishedAt = 1500L)

        val sorted = listOf(older, newer, mid)
            .filter { it.starred }
            .sortedByDescending { it.publishedAt ?: 0L }

        assertEquals("newest must be first", "ep-new", sorted[0].id)
        assertEquals("mid must be second", "ep-mid", sorted[1].id)
        assertEquals("oldest must be last", "ep-old", sorted[2].id)
    }

    // ── NAMESPACE constant ────────────────────────────────────────────────────

    @Test
    fun `NAMESPACE constant matches Rust PodcastActionModule NAMESPACE`() {
        assertEquals(
            "NAMESPACE must match Rust PodcastActionModule::NAMESPACE = \"podcast\"",
            "podcast",
            BookmarkActions.NAMESPACE,
        )
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    /** Construct a minimal [EpisodeSummary] with just the fields under test. */
    private fun makeEpisode(
        id: String,
        starred: Boolean,
        publishedAt: Long? = null,
    ): EpisodeSummary = EpisodeSummary(
        id = id,
        title = "Test Episode $id",
        starred = starred,
        publishedAt = publishedAt,
    )
}
