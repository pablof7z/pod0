package io.f7z.podcast

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Coverage for issue #320 — Android snapshot delivery is push-driven, not timed.
 *
 * `MainActivity` no longer runs a `while(true)/delay(500ms)` pull loop. It blocks
 * on `KernelBridge.nextUpdate()`, which returns the kernel's push frame as the
 * enveloped JSON `{"t":"snapshot","v":{...}}` (see
 * `apps/nmp-app-podcast/src/android.rs::on_update` +
 * `nmp_app_podcast_decode_update_frame`). These tests pin the decode contract
 * that makes that loop correct: the envelope unwraps to the same
 * `PodcastSnapshot` the bare projection pull yields, successive frames advance
 * `rev` (so the UI reacts to each emit), and non-snapshot / malformed frames are
 * dropped so a bad frame can never blank or crash the surface.
 */
class SnapshotCodecTest {

    @Test
    fun `push envelope unwraps to the same snapshot as a bare pull`() {
        // The bare projection payload `podcastSnapshot()` returns off the cache.
        val bare = """{"running":true,"rev":7,"schema_version":1,"toast":"hi"}"""
        // The identical projection wrapped in the push-frame envelope that
        // `nextUpdate()` returns after the kernel decodes its FlatBuffers frame.
        val enveloped = """{"t":"snapshot","v":$bare}"""

        val fromPull = SnapshotCodec.decode(bare)
        val fromPush = SnapshotCodec.decodeEnvelope(enveloped)

        assertNotNull(fromPull)
        assertNotNull(fromPush)
        // Push delivery must be wire-equivalent to the old pull — same state.
        assertEquals(fromPull, fromPush)
        assertEquals(7L, fromPush!!.rev)
        assertTrue(fromPush.running)
        assertEquals("hi", fromPush.toast)
    }

    @Test
    fun `successive push frames advance the revision`() {
        // Two frames the kernel would emit in sequence; the loop reacts to each.
        val frames = listOf(
            """{"t":"snapshot","v":{"running":true,"rev":1,"schema_version":1}}""",
            """{"t":"snapshot","v":{"running":true,"rev":2,"schema_version":1}}""",
        )

        val revs = frames.mapNotNull { SnapshotCodec.decodeEnvelope(it)?.rev }

        // Monotonically increasing rev proves snapshot changes propagate per
        // emit — no timer involved, one decoded snapshot per pushed frame.
        assertEquals(listOf(1L, 2L), revs)
    }

    @Test
    fun `non-snapshot envelope tag yields null`() {
        // D7 actor-death contract: the kernel can emit a panic frame instead of
        // a snapshot. The loop must drop it (keep the last good snapshot), not
        // mis-decode it as state.
        val panic = """{"t":"panic","message":"actor died"}"""

        assertNull(SnapshotCodec.decodeEnvelope(panic))
    }

    @Test
    fun `malformed or empty input yields null`() {
        assertNull(SnapshotCodec.decodeEnvelope(null))
        assertNull(SnapshotCodec.decodeEnvelope(""))
        assertNull(SnapshotCodec.decodeEnvelope("not json"))
        // A snapshot tag with no `v` payload is incomplete — dropped, not crashed.
        assertNull(SnapshotCodec.decodeEnvelope("""{"t":"snapshot"}"""))
    }

    @Test
    fun `bare decode still works for the first-paint pull`() {
        // MainActivity still does one bare pull for the initial frame before it
        // switches to the push loop; that path must keep decoding the bare shape.
        val snapshot = SnapshotCodec.decode("""{"running":true,"rev":3,"schema_version":1}""")

        assertNotNull(snapshot)
        assertEquals(3L, snapshot!!.rev)
    }

    @Test
    fun `feedback threads decode from snapshot projection`() {
        val raw = """
            {
              "running": true,
              "rev": 8,
              "schema_version": 1,
              "feedback_threads": [
                {
                  "event_id": "root1",
                  "author_pubkey": "alice",
                  "category": "bug",
                  "content": "Crash on launch",
                  "created_at": 100,
                  "title": "Launch crash",
                  "summary": "The app exits immediately.",
                  "status_label": "open",
                  "replies": [
                    {
                      "event_id": "reply1",
                      "author_pubkey": "bob",
                      "content": "Looking at it",
                      "created_at": 110
                    }
                  ]
                }
              ]
            }
        """.trimIndent()

        val snapshot = SnapshotCodec.decode(raw)

        assertNotNull(snapshot)
        val thread = snapshot!!.feedbackThreads.single()
        assertEquals("root1", thread.eventId)
        assertEquals("Launch crash", thread.title)
        assertEquals("open", thread.statusLabel)
        assertEquals("reply1", thread.replies.single().eventId)
    }
}
