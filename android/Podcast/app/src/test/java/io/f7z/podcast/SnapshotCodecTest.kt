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
 * on `KernelBridge.nextUpdate()`, which returns the kernel's push frame after
 * generated UniFFI forwards it through `PodcastApp.decodeUpdateFrame`. These
 * tests pin the decode contract that makes that loop correct: non-snapshot /
 * malformed frames are dropped so a bad frame can never blank or crash the
 * surface, and the bare pull path for the first-paint snapshot continues to
 * work.
 *
 * NOTE: As of NMP v0.5.0 (PR #404) the push-frame path uses per-domain typed
 * sidecars decoded via [SnapshotCodec.decodeDomainFrames] + [SnapshotCodec.mergeFrames].
 * The old `decodeEnvelope` (single full-snapshot decode from the push envelope)
 * no longer exists — the full-snapshot decode contract is now covered by
 * [DomainFrameWireTest]. The tests below focus on the bare pull path and
 * edge-case envelope handling via [SnapshotCodec.decodeDomainFrames].
 */
class SnapshotCodecTest {

    @Test
    fun `non-snapshot envelope tag yields null`() {
        // D7 actor-death contract: the kernel can emit a panic frame instead of
        // a snapshot. The loop must drop it (keep the last good snapshot), not
        // mis-decode it as state.
        val panic = """{"t":"panic","message":"actor died"}"""

        assertNull(SnapshotCodec.decodeDomainFrames(panic))
    }

    @Test
    fun `malformed or empty input yields null`() {
        assertNull(SnapshotCodec.decodeDomainFrames(null))
        assertNull(SnapshotCodec.decodeDomainFrames(""))
        assertNull(SnapshotCodec.decodeDomainFrames("not json"))
        // A snapshot tag with no `v` payload is incomplete — dropped, not crashed.
        assertNull(SnapshotCodec.decodeDomainFrames("""{"t":"snapshot"}"""))
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
    fun `active account decodes profile name and about`() {
        val snapshot = SnapshotCodec.decode(
            """
            {
              "running": true,
              "rev": 4,
              "schema_version": 1,
              "active_account": {
                "npub": "npub1alice",
                "pubkey_hex": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "fingerprint": "sha256:abc",
                "display_name": "Alice Example",
                "mode": "local",
                "picture_url": "https://example.test/avatar.png",
                "name": "alice",
                "about": "Builds shows"
              }
            }
            """.trimIndent()
        )

        assertNotNull(snapshot)
        val account = snapshot!!.activeAccount
        assertNotNull(account)
        assertEquals("alice", account!!.name)
        assertEquals("Builds shows", account.about)
    }

    @Test
    fun `local notes decode from snapshot projection`() {
        val snapshot = SnapshotCodec.decode(
            """
            {
              "running": true,
              "rev": 5,
              "schema_version": 1,
              "notes": [
                {
                  "id": "note-1",
                  "text": "Remember this",
                  "kind": "free",
                  "target": {
                    "type": "episode",
                    "episode_id": "ep-1",
                    "position_secs": 12.5
                  },
                  "created_at": 123,
                  "deleted": false,
                  "author": "user"
                }
              ]
            }
            """.trimIndent()
        )

        assertNotNull(snapshot)
        val note = snapshot!!.notes.single()
        assertEquals("note-1", note.id)
        assertEquals("Remember this", note.text)
        val target = note.target!!
        assertEquals("episode", target.type)
        assertEquals("ep-1", target.episodeId)
        assertEquals(12.5, target.positionSecs!!, 0.0)
    }

    @Test
    fun `friend local notes decode from snapshot projection`() {
        val snapshot = SnapshotCodec.decode(
            """
            {
              "running": true,
              "rev": 6,
              "schema_version": 1,
              "notes": [
                {
                  "id": "note-friend",
                  "text": "Ask Alice",
                  "kind": "free",
                  "target": {
                    "type": "friend",
                    "friend_id": "friend-1"
                  },
                  "created_at": 456,
                  "deleted": false,
                  "author": "user"
                }
              ]
            }
            """.trimIndent()
        )

        assertNotNull(snapshot)
        val target = snapshot!!.notes.single().target!!
        assertEquals("friend", target.type)
        assertEquals("friend-1", target.friendId)
    }

    @Test
    fun `friends decode from snapshot projection`() {
        val snapshot = SnapshotCodec.decode(
            """
            {
              "running": true,
              "rev": 7,
              "schema_version": 1,
              "friends": [
                {
                  "id": "friend-1",
                  "display_name": "Alice",
                  "pubkey_hex": "aabbcc",
                  "added_at": 123,
                  "avatar_url": "https://example.com/alice.png",
                  "about": "Builds shows"
                }
              ]
            }
            """.trimIndent()
        )

        assertNotNull(snapshot)
        val friend = snapshot!!.friends.single()
        assertEquals("friend-1", friend.id)
        assertEquals("Alice", friend.displayName)
        assertEquals("aabbcc", friend.pubkeyHex)
        assertEquals(123L, friend.addedAt)
        assertEquals("https://example.com/alice.png", friend.avatarUrl)
        assertEquals("Builds shows", friend.about)
    }

    @Test
    fun `nested local notes decode from snapshot projection`() {
        val snapshot = SnapshotCodec.decode(
            """
            {
              "running": true,
              "rev": 7,
              "schema_version": 1,
              "notes": [
                {
                  "id": "note-child",
                  "text": "Related",
                  "kind": "reflection",
                  "target": {
                    "type": "note",
                    "note_id": "note-parent"
                  },
                  "created_at": 789,
                  "deleted": false,
                  "author": "agent"
                }
              ]
            }
            """.trimIndent()
        )

        assertNotNull(snapshot)
        val target = snapshot!!.notes.single().target!!
        assertEquals("note", target.type)
        assertEquals("note-parent", target.noteId)
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

    @Test
    fun `successive push frames each yield distinct domain frames`() {
        // Two playback-only push frames the kernel would emit in sequence;
        // each must decode to a non-null PodcastDomainFrames independently.
        val frames = listOf(
            """{"t":"snapshot","v":{"running":true,"rev":1,"projections":{"podcast.playback":{"rev":1,"now_playing":null,"queue":[]}}}}""",
            """{"t":"snapshot","v":{"running":true,"rev":2,"projections":{"podcast.playback":{"rev":2,"now_playing":null,"queue":[]}}}}""",
        )

        val revs = frames
            .mapNotNull { SnapshotCodec.decodeDomainFrames(it) }
            .mapNotNull { it.playback?.rev }

        // Each emit produces a distinct PodcastDomainFrames with monotonically
        // increasing playback.rev — proves the push loop reacts per-emit.
        assertEquals(listOf(1L, 2L), revs)
    }

    @Test
    fun `social domain friends merge into snapshot`() {
        val raw = """
            {
              "t": "snapshot",
              "v": {
                "running": true,
                "rev": 8,
                "projections": {
                  "podcast.social": {
                    "rev": 8,
                    "social": null,
                    "nostr_conversations": [],
                    "friends": [
                      {
                        "id": "friend-1",
                        "display_name": "Alice",
                        "pubkey_hex": "aabbcc",
                        "added_at": 123
                      }
                    ]
                  }
                }
              }
            }
        """.trimIndent()

        val frames = SnapshotCodec.decodeDomainFrames(raw)
        assertNotNull(frames)
        val tracker = DomainRevTracker()
        val (snapshot, accepted) = SnapshotCodec.mergeFrames(frames!!, PodcastSnapshot(), tracker)

        assertTrue(accepted)
        val friend = snapshot.friends.single()
        assertEquals("friend-1", friend.id)
        assertEquals("Alice", friend.displayName)
        assertEquals("aabbcc", friend.pubkeyHex)
        assertEquals(8L, tracker.social)
    }
}
