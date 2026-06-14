package io.f7z.podcast

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Wire-fixture tests for the Android per-domain push-frame decode path (PR #404).
 *
 * All JSON fixtures are Rust-shaped: snake_case field names, exactly as
 * `nmp_app_podcast_decode_update_frame` injects them under
 * `v.projections[schema_id]` in the bridge envelope.
 *
 * This is the Android counterpart of iOS `KernelBridgeWireTests.swift` /
 * `KernelDomainMergeTests.swift`. A `@SerialName` typo or omission in
 * [DomainFrames.kt] will cause the relevant assertion here to fail, providing
 * early warning before a Rust field rename silently drops data on Android.
 *
 * Contract under test:
 *  1. Each domain decodes — snake_case wire keys map to Kotlin model fields.
 *  2. A playback-only frame does NOT clear the library slice (delta merge).
 *  3. Tombstone frames clear the corresponding domain slice.
 *  4. The drop-guard ignores a stale-rev domain frame (rev ≤ last-applied).
 *  5. A frame with no `podcast.*` domains → decodeDomainFrames returns null.
 */
class DomainFrameWireTest {

    // ── Fixture helpers ───────────────────────────────────────────────────────

    /** Wrap a projections map into the `{"t":"snapshot","v":{"projections":{...}}}` envelope. */
    private fun envelope(vararg pairs: Pair<String, String>): String {
        val projBody = pairs.joinToString(",") { (k, v) -> "\"$k\":$v" }
        return """{"t":"snapshot","v":{"rev":1,"running":true,"projections":{$projBody}}}"""
    }

    private val libraryFixture = """
        {
          "rev": 2,
          "library": [
            {
              "id": "pod-1",
              "title": "The Daily",
              "episode_count": 5,
              "unplayed_count": 3,
              "artwork_url": "https://example.com/art.jpg",
              "feed_url": "https://feeds.example.com/daily.xml",
              "episodes": [
                {
                  "id": "ep-1",
                  "title": "Episode One",
                  "podcast_id": "pod-1",
                  "podcast_title": "The Daily",
                  "duration_secs": 1800.0,
                  "played": false
                }
              ]
            }
          ],
          "categories": [],
          "search_results": [],
          "nostr_results": [],
          "owned_podcasts": [],
          "inbox": [
            {
              "episode_id": "ep-1",
              "episode_title": "Episode One",
              "podcast_id": "pod-1",
              "podcast_title": "The Daily",
              "published_at": 1717200000,
              "priority_score": 0.8,
              "priority_reason": "High relevance"
            }
          ],
          "inbox_triage_in_progress": true,
          "inbox_last_triaged_at": 1717200123
        }
    """.trimIndent()

    private val playbackFixture = """
        {
          "rev": 3,
          "now_playing": {
            "episode_id": "ep-1",
            "podcast_id": "pod-1",
            "episode_title": "Episode One",
            "podcast_title": "The Daily",
            "artwork_url": "https://example.com/art.jpg",
            "position_secs": 120.5,
            "duration_secs": 1800.0,
            "is_playing": true,
            "speed": 1.5,
            "volume": 0.9
          },
          "queue": [
            {
              "id": "ep-2",
              "title": "Episode Two",
              "podcast_id": "pod-1",
              "podcast_title": "The Daily"
            }
          ]
        }
    """.trimIndent()

    private val downloadsFixture = """
        {
          "rev": 4,
          "downloads": {
            "active": [
              {
                "episode_id": "ep-1",
                "url": "https://example.com/ep1.mp3",
                "progress": 0.45,
                "state": "active",
                "total_bytes": 50000000
              }
            ],
            "queued_count": 2,
            "completed_today": 1
          }
        }
    """.trimIndent()

    private val settingsFixture = """
        {
          "rev": 5,
          "settings": {
            "default_playback_rate": 1.25,
            "auto_delete_downloads_after_played": true,
            "agent_initial_model": "deepseek-v4-flash:cloud",
            "agent_initial_model_name": "DeepSeek Flash",
            "agent_thinking_model": "deepseek-v4-pro:cloud",
            "agent_thinking_model_name": "DeepSeek Pro",
            "memory_compilation_model": "deepseek-v4-flash:cloud",
            "memory_compilation_model_name": "DeepSeek Flash",
            "wiki_model": "deepseek-v4-flash:cloud",
            "wiki_model_name": "DeepSeek Flash",
            "categorization_model": "deepseek-v4-flash:cloud",
            "categorization_model_name": "DeepSeek Flash",
            "chapter_compilation_model": "deepseek-v4-flash:cloud",
            "chapter_compilation_model_name": "DeepSeek Flash",
            "embeddings_model": "deepseek-v4-flash:cloud",
            "embeddings_model_name": "DeepSeek Flash",
            "image_generation_model": "google/gemini-2.5-flash-image",
            "image_generation_model_name": "Gemini 2.5 Flash",
            "reranker_enabled": false,
            "open_router_credential_source": "",
            "open_router_key_present": false,
            "ollama_credential_source": "",
            "ollama_key_present": false,
            "ollama_chat_url": "https://ollama.com/api/chat",
            "eleven_labs_credential_source": "",
            "eleven_labs_key_present": false,
            "assembly_ai_credential_source": "",
            "assembly_ai_key_present": false,
            "perplexity_credential_source": "",
            "perplexity_key_present": false,
            "stt_provider": "apple_native",
            "effective_stt_provider": "apple_native",
            "effective_stt_provider_requires_key": false,
            "open_router_whisper_model": "openai/whisper-1",
            "assembly_ai_stt_model": "universal-3-pro,universal-2",
            "eleven_labs_stt_model": "scribe_v1",
            "eleven_labs_tts_model": "eleven_turbo_v2_5",
            "eleven_labs_voice_id": "",
            "eleven_labs_voice_name": ""
          },
          "configured_relays": []
        }
    """.trimIndent()

    private val identityFixture = """
        {
          "rev": 6,
          "active_account": {
            "npub": "npub1testuser",
            "pubkey_hex": "deadbeef1234",
            "display_name": "Test User",
            "mode": "local_key",
            "picture_url": "https://example.com/avatar.jpg"
          }
        }
    """.trimIndent()

    private val widgetFixture = """
        {
          "rev": 7,
          "widget": {
            "now_playing_episode_title": "Episode One",
            "now_playing_podcast_title": "The Daily",
            "now_playing_artwork_url": "https://example.com/art.jpg",
            "is_playing": true,
            "position_fraction": 0.33,
            "unplayed_count": 5
          }
        }
    """.trimIndent()

    private val miscFixture = """
        {
          "rev": 8,
          "agent_tasks": [
            {
              "id": "task-1",
              "title": "Generate summary",
              "intent_type": "summarize",
              "intent_label": "Summarize",
              "schedule": "daily",
              "status": "pending",
              "is_enabled": true
            }
          ],
          "feedback_threads": [],
          "feedback_events": [],
          "voice": {
            "is_speaking": false
          },
          "agent": {
            "messages": [],
            "is_busy": false
          },
          "wiki_articles": [],
          "wiki_search_results": [],
          "picks": [],
          "knowledge_search_results": [],
          "memory_facts": [],
          "clips": [],
          "comments": [],
          "agent_context": null
        }
    """.trimIndent()

    // ── 1. Per-domain snake_case decode contract ──────────────────────────────

    @Test
    fun `library domain decodes snake_case fields correctly`() {
        val raw = envelope("podcast.library" to libraryFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        assertNotNull("library frame must decode", frames)
        val lib = frames!!.library
        assertNotNull("library domain must be present", lib)
        assertEquals(2L, lib!!.rev)

        val show = lib.library!!.single()
        assertEquals("pod-1", show.id)
        assertEquals("The Daily", show.title)
        assertEquals(5, show.episodeCount)       // episode_count → episodeCount
        assertEquals(3, show.unplayedCount)      // unplayed_count → unplayedCount
        assertEquals("https://example.com/art.jpg", show.artworkUrl)  // artwork_url
        assertEquals("https://feeds.example.com/daily.xml", show.feedUrl) // feed_url

        val ep = show.episodes.single()
        assertEquals("ep-1", ep.id)
        assertEquals("Episode One", ep.title)
        assertEquals("pod-1", ep.podcastId)      // podcast_id → podcastId
        assertEquals("The Daily", ep.podcastTitle) // podcast_title → podcastTitle
        assertEquals(1800.0, ep.durationSecs!!, 0.001) // duration_secs

        // inbox decodes from the library domain
        val inboxItem = lib.inbox!!.single()
        assertEquals("ep-1", inboxItem.episodeId)       // episode_id
        assertEquals("Episode One", inboxItem.episodeTitle) // episode_title
        assertEquals("pod-1", inboxItem.podcastId)       // podcast_id
        assertEquals("The Daily", inboxItem.podcastTitle) // podcast_title
        assertEquals(1717200000L, inboxItem.publishedAt)   // published_at
        assertEquals(0.8f, inboxItem.priorityScore, 0.001f) // priority_score
        assertEquals("High relevance", inboxItem.priorityReason) // priority_reason

        // inbox_triage_in_progress
        assertTrue("inbox_triage_in_progress must decode", lib.inboxTriageInProgress!!)
        assertEquals(
            "inbox_last_triaged_at must decode",
            1717200123L,
            lib.inboxLastTriagedAt
        )

        // Other domains must be absent
        assertNull("playback must be absent in library-only frame", frames.playback)
        assertNull("identity must be absent in library-only frame", frames.identity)
    }

    @Test
    fun `playback domain decodes snake_case fields correctly`() {
        val raw = envelope("podcast.playback" to playbackFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        assertNotNull("playback frame must decode", frames)
        val play = frames!!.playback
        assertNotNull("playback domain must be present", play)
        assertEquals(3L, play!!.rev)

        val np = play.nowPlaying   // now_playing → nowPlaying
        assertNotNull("now_playing must decode", np)
        assertEquals("ep-1", np!!.episodeId)             // episode_id
        assertEquals("pod-1", np.podcastId)              // podcast_id
        assertEquals("Episode One", np.episodeTitle)     // episode_title
        assertEquals("The Daily", np.podcastTitle)        // podcast_title
        assertEquals("https://example.com/art.jpg", np.artworkUrl) // artwork_url
        assertEquals(120.5, np.positionSecs, 0.001)      // position_secs
        assertEquals(1800.0, np.durationSecs, 0.001)     // duration_secs
        assertTrue("is_playing must be true", np.isPlaying) // is_playing
        assertEquals(1.5f, np.speed, 0.001f)
        assertEquals(0.9f, np.volume, 0.001f)

        val queueItem = play.queue!!.single()
        assertEquals("ep-2", queueItem.id)
        assertEquals("Episode Two", queueItem.title)
        assertEquals("pod-1", queueItem.podcastId)       // podcast_id

        // Other domains must be absent
        assertNull("library must be absent in playback-only frame", frames.library)
    }

    @Test
    fun `downloads domain decodes snake_case fields correctly`() {
        val raw = envelope("podcast.downloads" to downloadsFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        assertNotNull("downloads frame must decode", frames)
        val dl = frames!!.downloads
        assertNotNull("downloads domain must be present", dl)
        assertEquals(4L, dl!!.rev)

        val snapshot = dl.downloads
        assertNotNull("downloads snapshot must be present", snapshot)
        assertEquals(2, snapshot!!.queuedCount)       // queued_count
        assertEquals(1, snapshot.completedToday)      // completed_today

        val active = snapshot.active.single()
        assertEquals("ep-1", active.episodeId)        // episode_id
        assertEquals("https://example.com/ep1.mp3", active.url)
        assertEquals(0.45f, active.progress, 0.001f)
        assertEquals("active", active.state)
        assertEquals(50_000_000L, active.totalBytes)  // total_bytes
    }

    @Test
    fun `settings domain decodes snake_case fields correctly`() {
        val raw = envelope("podcast.settings" to settingsFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        assertNotNull("settings frame must decode", frames)
        val sett = frames!!.settings
        assertNotNull("settings domain must be present", sett)
        assertEquals(5L, sett!!.rev)

        val s = sett.settings
        assertNotNull("settings snapshot must be present", s)
        assertEquals(1.25f, s!!.defaultPlaybackRate, 0.001f) // default_playback_rate
        assertTrue("auto_delete_downloads_after_played", s.autoDeleteDownloads)
        assertEquals("deepseek-v4-flash:cloud", s.agentInitialModel) // agent_initial_model
        assertEquals("DeepSeek Flash", s.agentInitialModelName)       // agent_initial_model_name
        assertEquals("deepseek-v4-pro:cloud", s.agentThinkingModel)   // agent_thinking_model
        assertFalse("reranker_enabled must be false", s.rerankerEnabled)
        assertFalse("open_router_key_present must be false", s.openRouterKeyPresent)
        assertEquals("apple_native", s.sttProvider)    // stt_provider
        assertEquals("apple_native", s.effectiveSttProvider) // effective_stt_provider
        assertFalse("effective_stt_provider_requires_key", s.effectiveSttProviderRequiresKey)
        assertEquals("openai/whisper-1", s.openRouterWhisperModel) // open_router_whisper_model
        assertEquals("scribe_v1", s.elevenLabsSttModel) // eleven_labs_stt_model
    }

    @Test
    fun `identity domain decodes snake_case active_account fields correctly`() {
        val raw = envelope("podcast.identity" to identityFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        assertNotNull("identity frame must decode", frames)
        val ident = frames!!.identity
        assertNotNull("identity domain must be present", ident)
        assertEquals(6L, ident!!.rev)

        val acct = ident.activeAccount   // active_account → activeAccount
        assertNotNull("active_account must decode", acct)
        assertEquals("npub1testuser", acct!!.npub)
        assertEquals("deadbeef1234", acct.pubkeyHex)    // pubkey_hex → pubkeyHex
        assertEquals("Test User", acct.displayName)     // display_name → displayName
        assertEquals("local_key", acct.mode)
        assertEquals("https://example.com/avatar.jpg", acct.pictureUrl) // picture_url
    }

    @Test
    fun `widget domain decodes snake_case fields correctly`() {
        val raw = envelope("podcast.widget" to widgetFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        assertNotNull("widget frame must decode", frames)
        val wid = frames!!.widget
        assertNotNull("widget domain must be present", wid)
        assertEquals(7L, wid!!.rev)

        val w = wid.widget
        assertNotNull("widget snapshot must decode", w)
        assertEquals("Episode One", w!!.nowPlayingEpisodeTitle)  // now_playing_episode_title
        assertEquals("The Daily", w.nowPlayingPodcastTitle)       // now_playing_podcast_title
        assertEquals("https://example.com/art.jpg", w.nowPlayingArtworkUrl) // now_playing_artwork_url
        assertTrue("is_playing must be true", w.isPlaying)  // is_playing
        assertEquals(0.33f, w.positionFraction, 0.001f)     // position_fraction
        assertEquals(5, w.unplayedCount)                     // unplayed_count
    }

    @Test
    fun `misc domain decodes snake_case fields correctly`() {
        val raw = envelope("podcast.misc" to miscFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        assertNotNull("misc frame must decode", frames)
        val misc = frames!!.misc
        assertNotNull("misc domain must be present", misc)
        assertEquals(8L, misc!!.rev)

        val task = misc.agentTasks!!.single()  // agent_tasks → agentTasks
        assertEquals("task-1", task.id)
        assertEquals("Generate summary", task.title)
        assertEquals("summarize", task.intentType)   // intent_type
        assertEquals("Summarize", task.intentLabel)  // intent_label
        assertEquals("daily", task.schedule)
        assertEquals("pending", task.status)
        assertTrue("is_enabled must be true", task.isEnabled)  // is_enabled

        val voice = misc.voice
        assertNotNull("voice must decode", voice)
        assertFalse("is_speaking must be false", voice!!.isSpeaking) // is_speaking

        val agent = misc.agent
        assertNotNull("agent must decode", agent)
        // AgentSnapshot carries messages + is_busy (Rust AgentSnapshot in agent.rs).
        // ConversationsSnapshot (active_count / latest_conversation_id) is a separate
        // struct reserved for the future multi-conversation surface and is NOT what
        // podcast.misc emits under the "agent" key.
        assertFalse("is_busy must be false", agent!!.isBusy)  // is_busy
        assertTrue("messages must be empty", agent.messages.isEmpty())  // messages
    }

    // ── 2. Playback-only frame does NOT clear the library slice ───────────────

    @Test
    fun `playback-only frame does not clobber library slice in mergeFrames`() {
        // Seed: merge a library frame to populate the composite.
        val libEnvelope = envelope("podcast.library" to libraryFixture)
        val libFrames = SnapshotCodec.decodeDomainFrames(libEnvelope)
        assertNotNull("library frame must decode", libFrames)

        val composite = PodcastSnapshot()
        val tracker = DomainRevTracker()
        val (afterLib, libAccepted) = SnapshotCodec.mergeFrames(libFrames!!, composite, tracker)
        assertTrue("library frame must be accepted", libAccepted)
        assertEquals(1, afterLib.library.size)
        assertEquals("The Daily", afterLib.library.single().title)
        assertEquals(1717200123L, afterLib.inboxLastTriagedAt)

        // Now merge a playback-only frame — library domain is absent.
        val playEnvelope = envelope("podcast.playback" to playbackFixture)
        val playFrames = SnapshotCodec.decodeDomainFrames(playEnvelope)
        assertNotNull("playback frame must decode", playFrames)
        assertNull("library domain must be absent in playback-only frame",
                   playFrames!!.library)

        val (afterPlay, playAccepted) = SnapshotCodec.mergeFrames(playFrames, afterLib, tracker)
        assertTrue("playback frame must be accepted", playAccepted)

        // Library slice must survive untouched.
        assertEquals(
            "library slice must NOT be cleared by a playback-only push frame",
            1,
            afterPlay.library.size
        )
        assertEquals("The Daily", afterPlay.library.single().title)

        // nowPlaying must be set from the playback domain.
        assertNotNull("nowPlaying must be set", afterPlay.nowPlaying)
        assertEquals("ep-1", afterPlay.nowPlaying!!.episodeId)
    }

    // ── 3. Tombstone frames clear the corresponding domain slice ──────────────

    @Test
    fun `library tombstone clears library slice in mergeFrames`() {
        // Seed composite with a library frame.
        val libFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.library" to libraryFixture))!!
        val tracker = DomainRevTracker()
        val (seeded, _) = SnapshotCodec.mergeFrames(libFrames, PodcastSnapshot(), tracker)
        assertEquals("Seeded library must have one show", 1, seeded.library.size)

        // Apply tombstone: rev=99, library=null.
        val tombstone = """{"rev":99,"library":null}"""
        val tombFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.library" to tombstone))
        assertNotNull("library tombstone frame must decode", tombFrames)
        assertNotNull("library domain must be non-null in tombstone frame",
                      tombFrames!!.library)
        assertNull("tombstone frame library payload must be null",
                   tombFrames.library!!.library)

        val (cleared, accepted) = SnapshotCodec.mergeFrames(tombFrames, seeded, tracker)
        assertTrue("tombstone must be accepted (rev=99 > rev=2)", accepted)
        assertEquals("library tombstone must clear composite.library to empty",
                     0, cleared.library.size)
        assertNull("library tombstone must clear inbox timestamp",
                   cleared.inboxLastTriagedAt)
        assertEquals(99L, tracker.library)
    }

    @Test
    fun `downloads tombstone clears downloads slice in mergeFrames`() {
        // Seed with a downloads frame.
        val dlFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.downloads" to downloadsFixture))!!
        val tracker = DomainRevTracker()
        val (seeded, _) = SnapshotCodec.mergeFrames(dlFrames, PodcastSnapshot(), tracker)
        assertNotNull("Seeded composite must have non-null downloads", seeded.downloads)

        // Tombstone: rev=99, downloads=null.
        val tombstone = """{"rev":99,"downloads":null}"""
        val tombFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.downloads" to tombstone))
        assertNotNull("downloads tombstone frame must decode", tombFrames)
        assertNull("tombstone frame downloads payload must be null",
                   tombFrames!!.downloads!!.downloads)

        val (cleared, accepted) = SnapshotCodec.mergeFrames(tombFrames, seeded, tracker)
        assertTrue("tombstone must be accepted", accepted)
        assertNull("downloads tombstone must clear composite.downloads to null",
                   cleared.downloads)
        assertEquals(99L, tracker.downloads)
    }

    @Test
    fun `identity tombstone clears activeAccount in mergeFrames`() {
        // Seed with an identity frame.
        val identFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.identity" to identityFixture))!!
        val tracker = DomainRevTracker()
        val (seeded, _) = SnapshotCodec.mergeFrames(identFrames, PodcastSnapshot(), tracker)
        assertNotNull("Seeded composite must have non-null activeAccount",
                      seeded.activeAccount)

        // Tombstone: rev=99, active_account=null.
        val tombstone = """{"rev":99,"active_account":null}"""
        val tombFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.identity" to tombstone))
        assertNotNull("identity tombstone frame must decode", tombFrames)
        assertNotNull("identity domain must be non-null in tombstone",
                      tombFrames!!.identity)
        assertNull("active_account must decode as null in tombstone",
                   tombFrames.identity!!.activeAccount)

        val (cleared, accepted) = SnapshotCodec.mergeFrames(tombFrames, seeded, tracker)
        assertTrue("tombstone must be accepted", accepted)
        assertNull("identity tombstone must clear composite.activeAccount",
                   cleared.activeAccount)
        assertEquals(99L, tracker.identity)
    }

    @Test
    fun `widget tombstone clears widget slice in mergeFrames`() {
        // Seed with a widget frame.
        val widFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.widget" to widgetFixture))!!
        val tracker = DomainRevTracker()
        val (seeded, _) = SnapshotCodec.mergeFrames(widFrames, PodcastSnapshot(), tracker)
        assertNotNull("Seeded composite must have non-null widget", seeded.widget)

        // Tombstone: rev=99, widget=null.
        val tombstone = """{"rev":99,"widget":null}"""
        val tombFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.widget" to tombstone))
        assertNotNull("widget tombstone frame must decode", tombFrames)
        assertNull("widget must decode as null in tombstone",
                   tombFrames!!.widget!!.widget)

        val (cleared, accepted) = SnapshotCodec.mergeFrames(tombFrames, seeded, tracker)
        assertTrue("tombstone must be accepted", accepted)
        assertNull("widget tombstone must clear composite.widget", cleared.widget)
        assertEquals(99L, tracker.widget)
    }

    // ── 4. Drop-guard ignores stale-rev domain frames ─────────────────────────

    @Test
    fun `drop-guard ignores stale-rev playback frame in mergeFrames`() {
        val tracker = DomainRevTracker()

        // Accept rev=5 frame with nowPlayingId="ep-current".
        val highRevPlayback = """
            {
              "rev": 5,
              "now_playing": {
                "episode_id": "ep-current",
                "podcast_id": "pod-1",
                "position_secs": 0.0,
                "duration_secs": 1800.0,
                "is_playing": true,
                "speed": 1.0,
                "volume": 1.0
              },
              "queue": []
            }
        """.trimIndent()
        val highFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.playback" to highRevPlayback))!!
        val (afterHigh, highAccepted) = SnapshotCodec.mergeFrames(
            highFrames, PodcastSnapshot(), tracker)
        assertTrue("high-rev frame must be accepted", highAccepted)
        assertEquals("ep-current", afterHigh.nowPlaying?.episodeId)
        assertEquals(5L, tracker.playback)

        // Stale rev=3 frame — must be dropped.
        val stalePlayback = """
            {
              "rev": 3,
              "now_playing": {
                "episode_id": "ep-stale",
                "podcast_id": "pod-1",
                "position_secs": 0.0,
                "duration_secs": 1800.0,
                "is_playing": false,
                "speed": 1.0,
                "volume": 1.0
              },
              "queue": []
            }
        """.trimIndent()
        val staleFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.playback" to stalePlayback))!!
        val (afterStale, staleAccepted) = SnapshotCodec.mergeFrames(
            staleFrames, afterHigh, tracker)
        assertFalse("stale-rev frame must NOT be accepted", staleAccepted)
        assertEquals(
            "composite must retain ep-current after stale-rev drop",
            "ep-current",
            afterStale.nowPlaying?.episodeId
        )
        // Tracker must NOT advance on a dropped frame.
        assertEquals(5L, tracker.playback)
    }

    @Test
    fun `drop-guard ignores equal-rev domain frame in mergeFrames`() {
        val tracker = DomainRevTracker()

        // Accept rev=10 identity frame.
        val identFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.identity" to identityFixture))!!   // rev=6
        val (seeded, _) = SnapshotCodec.mergeFrames(identFrames, PodcastSnapshot(), tracker)
        assertEquals("npub1testuser", seeded.activeAccount?.npub)
        assertEquals(6L, tracker.identity)

        // Same rev=6 again — equal rev must be dropped (not strictly-greater).
        val dupFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.identity" to identityFixture))!!
        val (afterDup, dupAccepted) = SnapshotCodec.mergeFrames(dupFrames, seeded, tracker)
        assertFalse("equal-rev frame must be dropped by the drop-guard", dupAccepted)
        // State unchanged.
        assertEquals("npub1testuser", afterDup.activeAccount?.npub)
        assertEquals(6L, tracker.identity)
    }

    // ── 5. No podcast.* domains → decodeDomainFrames returns null (D6) ────────

    @Test
    fun `frame with no podcast-star domains returns null from decodeDomainFrames`() {
        // Envelope with an unrelated non-podcast projection.
        val noOpFrame = """{"t":"snapshot","v":{"rev":1,"running":true,"projections":{"some.other.domain":{"rev":1}}}}"""
        assertNull(
            "frame with no podcast.* domains must return null (D6 degrade)",
            SnapshotCodec.decodeDomainFrames(noOpFrame)
        )
    }

    @Test
    fun `empty projections map returns null from decodeDomainFrames`() {
        val emptyProjections = """{"t":"snapshot","v":{"rev":1,"running":true,"projections":{}}}"""
        assertNull(
            "frame with empty projections must return null",
            SnapshotCodec.decodeDomainFrames(emptyProjections)
        )
    }

    @Test
    fun `non-snapshot envelope tag returns null from decodeDomainFrames`() {
        val panicFrame = """{"t":"panic","message":"actor died"}"""
        assertNull(
            "non-snapshot frame tag must return null",
            SnapshotCodec.decodeDomainFrames(panicFrame)
        )
    }

    @Test
    fun `null and malformed input returns null from decodeDomainFrames`() {
        assertNull(SnapshotCodec.decodeDomainFrames(null))
        assertNull(SnapshotCodec.decodeDomainFrames(""))
        assertNull(SnapshotCodec.decodeDomainFrames("not json"))
        assertNull(SnapshotCodec.decodeDomainFrames("""{"t":"snapshot"}"""))
    }

    // ── 6. Multi-domain frame decodes all present domains ─────────────────────

    @Test
    fun `multi-domain frame decodes all present domains independently`() {
        val raw = envelope(
            "podcast.library" to libraryFixture,
            "podcast.playback" to playbackFixture,
            "podcast.settings" to settingsFixture,
        )
        val frames = SnapshotCodec.decodeDomainFrames(raw)
        assertNotNull("multi-domain frame must decode", frames)
        assertNotNull("library domain must be present", frames!!.library)
        assertNotNull("playback domain must be present", frames.playback)
        assertNotNull("settings domain must be present", frames.settings)
        assertNull("identity domain must be absent", frames.identity)
        assertNull("widget domain must be absent", frames.widget)
        assertNull("downloads domain must be absent", frames.downloads)
        assertNull("misc domain must be absent", frames.misc)
        assertTrue("hasAnyDomain must be true", frames.hasAnyDomain)
    }

    // ── 7. Social domain decodes snake_case nostr_conversations fields ────────

    private val socialFixture = """
        {
          "rev": 9,
          "social": null,
          "nostr_conversations": [
            {
              "root_event_id": "deadbeef001",
              "counterparty_hex": "aabbccdd001",
              "participants": ["aabbccdd001", "11223344001"],
              "trusted": true,
              "first_seen": 1717200000,
              "last_activity": 1717286400,
              "turns": [
                {
                  "event_id": "evt-001",
                  "direction": "inbound",
                  "pubkey_hex": "aabbccdd001",
                  "created_at": 1717200001,
                  "content": "Hello from Nostr"
                },
                {
                  "event_id": "evt-002",
                  "direction": "outbound",
                  "pubkey_hex": "11223344001",
                  "created_at": 1717200100,
                  "content": "Reply from agent"
                }
              ]
            }
          ]
        }
    """.trimIndent()

    @Test
    fun `social domain decodes snake_case nostr_conversations correctly`() {
        val raw = envelope("podcast.social" to socialFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        assertNotNull("social frame must decode", frames)
        val soc = frames!!.social
        assertNotNull("social domain must be present", soc)
        assertEquals(9L, soc!!.rev)

        // social = null → tombstone shape for the follow-graph slice.
        assertNull("social field must be null (tombstone)", soc.social)

        val convos = soc.nostrConversations
        assertNotNull("nostr_conversations must decode", convos)
        assertEquals(1, convos!!.size)

        val convo = convos.single()
        assertEquals("deadbeef001", convo.rootEventId)    // root_event_id
        assertEquals("aabbccdd001", convo.counterpartyHex) // counterparty_hex
        assertTrue("trusted must be true", convo.trusted)
        assertEquals(1717200000L, convo.firstSeen)         // first_seen
        assertEquals(1717286400L, convo.lastActivity)      // last_activity
        assertEquals(2, convo.participants.size)

        val turns = convo.turns
        assertEquals(2, turns.size)

        val inbound = turns[0]
        assertEquals("evt-001", inbound.eventId)           // event_id
        assertEquals("inbound", inbound.direction)
        assertEquals("aabbccdd001", inbound.pubkeyHex)     // pubkey_hex
        assertEquals(1717200001L, inbound.createdAt)       // created_at
        assertEquals("Hello from Nostr", inbound.content)

        val outbound = turns[1]
        assertEquals("evt-002", outbound.eventId)
        assertEquals("outbound", outbound.direction)
        assertEquals("Reply from agent", outbound.content)

        // Other domains must be absent
        assertNull("library must be absent in social-only frame", frames.library)
        assertNull("misc must be absent in social-only frame", frames.misc)
    }

    @Test
    fun `social-only frame does not clobber other domains in mergeFrames`() {
        // Seed composite with a library frame.
        val libFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.library" to libraryFixture))!!
        val tracker = DomainRevTracker()
        val (seeded, _) = SnapshotCodec.mergeFrames(libFrames, PodcastSnapshot(), tracker)
        assertEquals(1, seeded.library.size)

        // Apply social-only frame — library domain must survive untouched.
        val socFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.social" to socialFixture))!!
        assertNotNull("social domain must be present", socFrames.social)
        assertNull("library domain must be absent in social-only frame", socFrames.library)

        val (afterSoc, socAccepted) = SnapshotCodec.mergeFrames(socFrames, seeded, tracker)
        assertTrue("social frame must be accepted", socAccepted)
        assertEquals(
            "library slice must NOT be cleared by a social-only push frame",
            1,
            afterSoc.library.size
        )
        assertEquals(9L, tracker.social)
    }

    // ── 7b. social mergeFrames populates PodcastSnapshot.nostrConversations ────
    //
    // These tests guard the UI-binding contract: the conversations list screen
    // binds `snapshot.nostrConversations`; this section verifies that
    // mergeFrames correctly wires the decoded SocialDomainFrame into that field
    // (the new code path added in feat/android-nostr-conversations).

    @Test
    fun `social mergeFrames populates nostrConversations on PodcastSnapshot`() {
        // Feed a podcast.social frame with one conversation.
        val raw = envelope("podcast.social" to socialFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)
        assertNotNull("social frame must decode", frames)

        val tracker = DomainRevTracker()
        val (snap, accepted) = SnapshotCodec.mergeFrames(frames!!, PodcastSnapshot(), tracker)

        assertTrue("social frame must be accepted", accepted)

        // The UI-binding field — asserts the merge wiring is complete.
        assertEquals(
            "nostrConversations must have one entry after mergeFrames",
            1,
            snap.nostrConversations.size,
        )

        val conv = snap.nostrConversations.single()
        // @SerialName contract: root_event_id, counterparty_hex, etc.
        assertEquals("deadbeef001", conv.rootEventId)
        assertEquals("aabbccdd001", conv.counterpartyHex)
        assertTrue("trusted must be true", conv.trusted)
        assertEquals(1717200000L, conv.firstSeen)
        assertEquals(1717286400L, conv.lastActivity)
        assertEquals(2, conv.turns.size)

        // Turns — direction + content (what the conversations detail screen renders)
        val inbound = conv.turns[0]
        assertEquals("inbound", inbound.direction)
        assertEquals("Hello from Nostr", inbound.content)
        assertEquals("aabbccdd001", inbound.pubkeyHex)
        assertEquals(1717200001L, inbound.createdAt)

        val outbound = conv.turns[1]
        assertEquals("outbound", outbound.direction)
        assertEquals("Reply from agent", outbound.content)
    }

    @Test
    fun `social tombstone clears nostrConversations in mergeFrames`() {
        // Seed composite with a social frame.
        val socFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.social" to socialFixture))!!
        val tracker = DomainRevTracker()
        val (seeded, _) = SnapshotCodec.mergeFrames(socFrames, PodcastSnapshot(), tracker)
        assertEquals("seeded nostrConversations must have one entry", 1,
            seeded.nostrConversations.size)

        // Tombstone: rev=99, nostr_conversations=null.
        val tombstone = """{"rev":99,"nostr_conversations":null}"""
        val tombFrames = SnapshotCodec.decodeDomainFrames(
            envelope("podcast.social" to tombstone))
        assertNotNull("social tombstone frame must decode", tombFrames)
        assertNotNull("social domain must be non-null in tombstone frame",
            tombFrames!!.social)
        assertNull("nostr_conversations must decode as null in tombstone",
            tombFrames.social!!.nostrConversations)

        val (cleared, accepted) = SnapshotCodec.mergeFrames(tombFrames, seeded, tracker)
        assertTrue("tombstone must be accepted (rev=99 > rev=9)", accepted)
        assertEquals(
            "nostrConversations tombstone must clear composite to empty list",
            0,
            cleared.nostrConversations.size,
        )
        assertEquals(99L, tracker.social)
    }

    // ── 8. DomainSchema constants match Rust schema IDs ───────────────────────

    @Test
    fun `DomainSchema constants match Rust kernel schema IDs`() {
        assertEquals("podcast.library",   DomainSchema.LIBRARY)
        assertEquals("podcast.playback",  DomainSchema.PLAYBACK)
        assertEquals("podcast.downloads", DomainSchema.DOWNLOADS)
        assertEquals("podcast.settings",  DomainSchema.SETTINGS)
        assertEquals("podcast.identity",  DomainSchema.IDENTITY)
        assertEquals("podcast.widget",    DomainSchema.WIDGET)
        assertEquals("podcast.social",    DomainSchema.SOCIAL)
        assertEquals("podcast.misc",      DomainSchema.MISC)
    }

    // ── 9. resolved_profiles — NMP-level projection decode + additive merge ────
    //
    // `projections["resolved_profiles"]` lives at the NMP top level (not inside
    // any `podcast.*` sub-domain). The Android bridge decodes it additively in
    // `decodeDomainFrames` so each push frame enriches the running profile map
    // without clobbering existing entries.
    //
    // Contract under test:
    //  a. ResolvedProfile @SerialName fields: display_name → display,
    //     picture_url → pictureUrl (no auto camelCase on Android).
    //  b. Map<String, ResolvedProfile> round-trips from the wire fixture.
    //  c. mergeFrames additively merges resolved_profiles (new entries win,
    //     existing entries are retained when absent from the incoming frame).
    //  d. An envelope with ONLY resolved_profiles (no podcast.* keys) still
    //     returns a non-null PodcastDomainFrames with resolvedProfiles populated
    //     — confirming decodeDomainFrames does not require a podcast.* domain.

    private val resolvedProfilesFixture = """
        {
          "aabbccdd001": {
            "display_name": "Alice Nostr",
            "picture_url": "https://example.com/alice.jpg"
          },
          "11223344001": {
            "display_name": "Bob Relay",
            "picture_url": "https://example.com/bob.png"
          }
        }
    """.trimIndent()

    @Test
    fun `ResolvedProfile decodes snake_case fields correctly`() {
        // resolved_profiles lives under projections["resolved_profiles"] in the
        // NMP-level envelope alongside podcast.* domains.
        val raw = envelope("resolved_profiles" to resolvedProfilesFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)

        // Even without a podcast.* key the bridge should accept the frame when
        // resolved_profiles is non-empty (decodeDomainFrames returns non-null).
        assertNotNull("frames must not be null when resolved_profiles is present", frames)

        val profiles = frames!!.resolvedProfiles
        assertEquals("must have 2 resolved profiles", 2, profiles.size)

        val alice = profiles["aabbccdd001"]
        assertNotNull("Alice's profile must be present", alice)
        assertEquals("display_name must map to display field", "Alice Nostr", alice!!.display)
        assertEquals("picture_url must map to pictureUrl field",
            "https://example.com/alice.jpg", alice.pictureUrl)

        val bob = profiles["11223344001"]
        assertNotNull("Bob's profile must be present", bob)
        assertEquals("Bob Relay", bob!!.display)
        assertEquals("https://example.com/bob.png", bob.pictureUrl)
    }

    @Test
    fun `resolved_profiles decodes partially-populated profile (missing fields are null)`() {
        // Kernel may omit display_name or picture_url for a profile with no metadata.
        val partial = """{"aabbccdd002":{}}"""
        val raw = envelope("resolved_profiles" to partial)
        val frames = SnapshotCodec.decodeDomainFrames(raw)
        assertNotNull("frames must not be null", frames)

        val profile = frames!!.resolvedProfiles["aabbccdd002"]
        assertNotNull("partial profile must be present", profile)
        assertNull("missing display_name must decode as null", profile!!.display)
        assertNull("missing picture_url must decode as null", profile.pictureUrl)
    }

    @Test
    fun `mergeFrames additively merges resolved_profiles (new entries win, old entries survive)`() {
        // Seed: frame with Alice only.
        val aliceOnly = """{"aabbccdd001":{"display_name":"Alice Nostr","picture_url":"https://example.com/alice.jpg"}}"""
        val frames1 = SnapshotCodec.decodeDomainFrames(envelope("resolved_profiles" to aliceOnly))
        assertNotNull("first frame must decode", frames1)

        val tracker = DomainRevTracker()
        val (snap1, accepted1) = SnapshotCodec.mergeFrames(frames1!!, PodcastSnapshot(), tracker)
        assertTrue("first frame must be accepted", accepted1)
        assertEquals("snap1 must have 1 resolved profile", 1, snap1.resolvedProfiles.size)
        assertEquals("Alice Nostr", snap1.resolvedProfiles["aabbccdd001"]?.display)

        // Second frame: Bob added, Alice absent (additive — Alice must survive).
        val bobOnly = """{"11223344001":{"display_name":"Bob Relay","picture_url":"https://example.com/bob.png"}}"""
        val frames2 = SnapshotCodec.decodeDomainFrames(envelope("resolved_profiles" to bobOnly))
        assertNotNull("second frame must decode", frames2)
        val (snap2, accepted2) = SnapshotCodec.mergeFrames(frames2!!, snap1, tracker)
        assertTrue("second frame must be accepted", accepted2)

        assertEquals("snap2 must have 2 resolved profiles after additive merge",
            2, snap2.resolvedProfiles.size)
        // Alice must still be present (additive merge does NOT clear absent keys).
        assertEquals("Alice Nostr", snap2.resolvedProfiles["aabbccdd001"]?.display)
        // Bob must be present (newly delivered key).
        assertEquals("Bob Relay", snap2.resolvedProfiles["11223344001"]?.display)
    }

    @Test
    fun `mergeFrames resolved_profiles update overwrites existing entry for same pubkey`() {
        // Seed: Alice with old display name.
        val aliceV1 = """{"aabbccdd001":{"display_name":"Alice Old","picture_url":"https://example.com/old.jpg"}}"""
        val frames1 = SnapshotCodec.decodeDomainFrames(envelope("resolved_profiles" to aliceV1))
        assertNotNull("first frame must decode", frames1)
        val tracker = DomainRevTracker()
        val (snap1, _) = SnapshotCodec.mergeFrames(frames1!!, PodcastSnapshot(), tracker)
        assertEquals("Alice Old", snap1.resolvedProfiles["aabbccdd001"]?.display)

        // Second frame: same pubkey with updated display name.
        val aliceV2 = """{"aabbccdd001":{"display_name":"Alice Updated","picture_url":"https://example.com/new.jpg"}}"""
        val frames2 = SnapshotCodec.decodeDomainFrames(envelope("resolved_profiles" to aliceV2))
        assertNotNull("second frame must decode", frames2)
        val (snap2, accepted2) = SnapshotCodec.mergeFrames(frames2!!, snap1, tracker)
        assertTrue("update frame must be accepted", accepted2)

        // The newer entry for the same key must win.
        assertEquals("updated entry must overwrite old entry for same pubkey",
            "Alice Updated", snap2.resolvedProfiles["aabbccdd001"]?.display)
        assertEquals("https://example.com/new.jpg",
            snap2.resolvedProfiles["aabbccdd001"]?.pictureUrl)
    }

    @Test
    fun `resolved_profiles and podcast-star domain coexist in same envelope`() {
        // The kernel can emit both resolved_profiles AND podcast.social in one push.
        val raw = envelope(
            "podcast.social" to socialFixture,
            "resolved_profiles" to resolvedProfilesFixture,
        )
        val frames = SnapshotCodec.decodeDomainFrames(raw)
        assertNotNull("combined frame must decode", frames)
        assertNotNull("social domain must decode", frames!!.social)
        assertEquals("2 resolved profiles must decode alongside social domain",
            2, frames.resolvedProfiles.size)
        assertEquals("Alice Nostr", frames.resolvedProfiles["aabbccdd001"]?.display)
    }

    @Test
    fun `resolved_profiles absent from envelope yields empty map (no NPE)`() {
        // A library-only frame has no resolved_profiles key — map must be empty, not null.
        val raw = envelope("podcast.library" to libraryFixture)
        val frames = SnapshotCodec.decodeDomainFrames(raw)
        assertNotNull("library frame must decode", frames)
        assertTrue("resolvedProfiles must be empty when absent from envelope",
            frames!!.resolvedProfiles.isEmpty())
    }
}
