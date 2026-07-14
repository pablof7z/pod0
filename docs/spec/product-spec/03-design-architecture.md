# Product Spec: Design and Architecture

> Part of the Podcastr product spec. Start at [PRODUCT_SPEC.md](../PRODUCT_SPEC.md).

## 6. Liquid Glass Design System (Visual + Motion + Haptics + Sound)

> Source: [docs/spec/briefs/ux-15-liquid-glass-system.md](briefs/ux-15-liquid-glass-system.md)

This is the **ground truth** for every surface designer. If a token is not here (or in the brief), it does not exist. If a surface contradicts this, the surface is wrong.

### 6.1 Five-tier material system

| Tier | API | Use |
|---|---|---|
| **T0 Hairline** | none — solid `bg.elevated` + 0.5 pt hairline | Pure reading surfaces (transcript, wiki body) — glass would distract |
| **T1 Clear** | `.glassEffect(.regular, in: rect)` | Default toolbars, segment controls, secondary chips |
| **T2 Tinted** | `.glassEffect(.regular.tint(c), in: rect)` | Mini-bar, agent reply bubble, friend incoming |
| **T3 Interactive** | `.glassEffect(.regular.tint(c).interactive())` | Buttons, agent orb, draggable scrubber thumb |
| **T4 Cinematic** | `GlassEffectContainer` + tinted children + parallax | Now Playing full screen, voice mode, briefing player |

**Rules.** Always wrap multiple T2/T3 elements in `GlassEffectContainer(spacing:)` — required for morph and perf (default spacing 24 pt; bump to 40 pt when elements should *not* merge). Never stack T2 over T2 (second blur turns to mud — use T0 underneath). Refraction is automatic in iOS 26 — do not fake it with manual gradients. Use the system's auto light/dark adaptation; do not hardcode opacities. Edge corners come from the corner scale: `Corner.lg` (16) cards, `Corner.xl` (24) sheets, `Corner.bubble` (18) chat bubbles, `Corner.pill` (14) chips — never custom values.

### 6.2 Identity tints — the three signals must be distinguishable in 200 ms peripheral vision

| Role | Light | Dark | Used for |
|---|---|---|---|
| `accent.player` | `#E94B2B` | `#FF6A4A` | **Copper — exclusive to Now-Playing surfaces.** Mini-bar progress line, full-player chrome, `playerOrb` button, home-screen mini-thumbnail badge. Nothing else. |
| `accent.agent` | `#5B3FE0`→`#2872F0` gradient | `#7A5BFF`→`#4D8FFF` | **Electric indigo→azure — agent identity.** Orb, agent-CTA buttons, agent message tint, voice-mode backdrop. |
| `accent.wiki` | `#1F6E55` | `#46C29A` | **Moss — knowledge surfaces.** Wiki citations, leaf glyph. |
| `accent.friend` | `#D9892F` | `#F2B45C` | **Amber — Nostr friend / friend-agent action.** 2 pt amber seam on the leading edge of any element initiated by a friend. |
| `accent.live` | `#C72D4D` | `#FF5577` | Recording / "agent listening" — signal red. |

**Rule of mutual exclusion.** A card cannot be both *from a friend* and *agent-generated*. If the agent forwards a friend's message, the bubble is friend (amber), with a small agent orb badge.

### 6.3 Typography

- **Primary face: SF Pro (system).** SF Pro Rounded reserved for chips, badges, and the agent voice (carries the "warm" register).
- **Editorial display: New York (system serif)** for hero titles only — episode titles on Now Playing, wiki article titles, briefing intros, agent prose in chat. Reserved for sizes ≥19 pt; loses character below.
- **Mono: SF Mono** for timestamps and code.
- Tokens: `displayHero` (NY 34/38), `displayLarge` (NY 28/32), `titleLg` (SF 22/26), `headline` (SF Rounded 17/22), `body` (SF 17/24), `caption` (SF 13/17), `monoTimestamp` (SF Mono 13/17).
- **Dynamic Type to AX5.** Every token must scale.

### 6.4 Motion language — "motion communicates causality"

| Curve | Spec | Use |
|---|---|---|
| `motion.snappy` | `spring(duration: 0.22, bounce: 0.12)` | Press feedback, chip toggles, scrubber ticks |
| `motion.standard` | `spring(duration: 0.35, bounce: 0.15)` | Default — sheet open, card expand, glass morph |
| `motion.considered` | `spring(duration: 0.55, bounce: 0.10)` | Now-playing transitions, agent surface entrance |
| `motion.cinematic` | `spring(duration: 0.85, bounce: 0.05)` | Full-screen player open, voice-mode entrance |
| `motion.bouncy` | `spring(duration: 0.45, bounce: 0.32)` | Celebratory only (briefing complete, save) |
| `motion.linear` | `linear(duration: continuous)` | Scrubbers, progress bars, waveform draw |

**Choreography rules.** Stagger don't simultaneous (40–60 ms between elements, max 5; beyond that, fade the group). Out before in (outgoing finishes 80 % of exit before incoming starts). Hero anchors share `matchedGeometryEffect` + `glassEffectID`; everything else cross-fades. Parallax: artwork on Now Playing scrolls at 0.6×, transcript at 1.0×, max delta 24 pt. **Scrubbing is linear, never spring** — springs feel laggy on continuous user input. Glass merges only inside containers — outside `GlassEffectContainer` glass elements cross-fade rather than morph.

### 6.5 Haptic + sound vocabulary

Extends existing `Haptics.swift` (do not restructure). New patterns: `playStart`, `playPause`, `scrubTick`, `agentListenStart`, `agentSpeakStart`, `agentInterrupt`, `bargeAccepted`, `clipMarked`, `friendIncoming`, `briefingStart`, `briefingComplete`. All cues are short (≤450 ms), -18 LUFS, ducked under any active audio, with a "subtle" 50 % gain variant for the in-podcast experience.

Signature sound cues: `agent.listen.up` (soft inhale, two-tone rising G→D, 280 ms), `agent.speak.in` (warm fade-in chime D5, 220 ms), `agent.barge` (brief reverse-swell descending, 180 ms), `transcribe.done` (two-note arpeggio A4→E5, 320 ms), `briefing.intro` (editorial signature, 4-note ascending, 1.4 s), `friend.knock` (two soft taps warm, 240 ms).

**Rule.** Never play a sound *and* fire a haptic for the same event unless explicitly listed; the body double-counts.

### 6.6 Accessibility constants

Every surface must pass: Dynamic Type to AX5 (single column at AX3+; eyebrow stacks at AX4+); WCAG AA 4.5:1 on body text against worst-case wallpaper; Reduce Motion (springs → cross-fades, breath rhythms → static states); Reduce Transparency (T2/T3 → solid `surfaceElevated` + 1 pt hairline, tints preserved); color independence (state expressed by shape *and* color, never color alone); 44 × 44 pt minimum hit targets with 8 pt slop on chips; haptic-only fallback for every audio cue.

---

## 7. Technical Architecture

> Source: [docs/spec/research/template-architecture-and-extension-plan.md](research/template-architecture-and-extension-plan.md), [docs/spec/research/skeleton-bootstrap-report.md](research/skeleton-bootstrap-report.md)

### 7.1 What we inherit from the template (do not rebuild)

**Already shipping in the renamed Podcastr skeleton:**

- **Entry & app lifecycle.** `AppMain.swift` (`PodcastrApp` `@main`), `App/RootView.swift` (TabView with Today / Library / Wiki / Ask / Home / Settings), `App/AppDelegate.swift` (deep-link routing, notification action buttons, shake handler).
- **Domain models.** `Item`, `Note`, `Friend`, `AgentMemory`, `Anchor` (discriminated union), `Settings`, `AgentActivity`, `NostrPendingApproval`. All `Codable + Sendable`; every decoder uses `decodeIfPresent` for forward-compat.
- **State.** `State/AppStateStore.swift` plus six extension files (`+Items`, `+Notes`, `+Memories`, `+Friends`, `+Nostr`, `+AgentActivity`, `+DerivedViews`). `@MainActor @Observable`. Single source of truth.
- **Persistence.** `State/Persistence.swift` encodes the entire `AppState` as JSON, writes to App Group `UserDefaults` keyed `podcastr.state.v1`. `iCloudSettingsSync` already merges arbitrary `Settings` fields key-by-key.
- **Agent loop.** `Features/Agent/AgentChatSession.swift` plus `AgentOpenRouterClient.swift` runs the SSE streaming loop with up to 20 turns. `Agent/AgentTools.swift` + `AgentToolSchema.swift` + `AgentPrompt.swift`, with tool dispatchers split into `+Items`, `+NotesMemory`, `+Reminders`, `+DueDates`, `+Search`. `AgentRelayBridge.swift` runs the same loop at 8-turn cap for inbound Nostr DMs.
- **Nostr subsystem.** `NostrRelayService` (WebSocket + kind-1 + reconnect), `NostrKeyPair` (P256K), `Bech32`, ACL (`nostrAllowedPubkeys` / `nostrBlockedPubkeys` / `nostrPendingApprovals`). The whole subsystem is kept verbatim.
- **Services.** `KeychainStore`, `OpenRouterCredentialStore`, `ElevenLabsCredentialStore`, `NostrCredentialStore`, `BYOKConnectService` (PKCE), `NotificationService`, `BadgeManager`, `SpotlightIndexer`, `iCloudSettingsSync`, `DataExport`, `DeepLinkHandler`, `VoiceItemService` (`SFSpeechRecognizer` dictation, harden for full-duplex), `ChatHistoryStore`, `ReviewPrompt`, `UserIdentityStore`.
- **Design.** `AppTheme` (split by concern), `GlassSurface` (calls native iOS 26 `.glassEffect()`), `Haptics`, `PressableStyle`, `ShakeDetector`, `MarkdownView`, `AsyncButton`.
- **Feedback.** Shake → `FeedbackWorkflow` state machine, `FeedbackStore` in `Documents/feedback_threads.json`. Wire `FeedbackView.performSubmission` to a backend later; that hook exists.
- **Build & CI.** `Project.swift` (iOS 26 deployment target, Swift 6 strict concurrency, App Group `group.com.podcastr.app`, bundle ID `io.f7z.podcast`, widget bundle ID `io.f7z.podcast.widget`, URL scheme `podcastr://`); `.github/workflows/{test,testflight}.yml`; `ci_scripts/`.

The skeleton has empty stubs at `App/Sources/{Audio/AudioEngine, Briefing/BriefingComposer, Knowledge/{VectorIndex, WikiPage}, Podcast/{PodcastSubscription, Episode}, Transcript/Transcript, Voice/AudioConversationManager}.swift` and feature view stubs at `Features/{Today, Library, Wiki, AgentChat (AskAgentView), EpisodeDetail, Player, Voice, Briefings, Search (PodcastSearchView)}/`. **All net-new code lands inside these stubs or alongside them.**

### 7.2 New modules

| Module | Files | Purpose |
|---|---|---|
| `Audio/` | `AudioSessionCoordinator`, `PlaybackEngine` (+`+RemoteCommands`, `+NowPlaying`), `NowPlayingMetadataPublisher`, `RemoteCommandHandler`, `AudioRouteObserver` | Single owner of every `AVAudioSession` transition. AVPlayer wrapper. `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter`. |
| `Podcast/` | `Subscription`, `Episode`, `RSSFeedParser`, `OPMLImporter`, `EnclosureDownloader`, `FeedRefreshScheduler` | RSS / OPML / Podcast Index / iTunes Search; `BGAppRefreshTask` scheduling. |
| `Transcript/` | `TranscriptChunk`, `TranscriptSource` enum, `PublisherTranscriptFetcher`, `ScribeTranscriptionClient` (ElevenLabs Scribe v2 batch), `TranscriptChunker` | Publisher-first → Scribe webhook fallback. |
| `Knowledge/` | `WikiPage`, `WikiCompiler` (`BGProcessingTask`), `EmbeddingService`, `VectorStore` protocol + `SQLiteVecStore`, `RAGQueryService` | LLM-wiki compile, embeddings via OpenRouter, sqlite-vec hybrid search. |
| `Voice/` | `AudioConversationManager` (state machine: idle → listening → thinking → speaking → bargeIn → listening) (+`+BargeIn`, `+TTSPlayback`), `BargeInDetector`, `TTSStreamer` (ElevenLabs Flash v2.5 WebSocket) | Conversational voice with sub-second barge-in. |
| `Briefing/` | `BriefingComposer` (script with `<beat>` markers + episode anchors), `BriefingScript`, `BriefingPlayer` (chains TTS clips; interrupt + resume to nearest `<beat>`) | Generate, stitch, play, branch. |

**File-size discipline.** AGENTS.md sets soft 300 / hard 500 lines. Follow the existing extension-per-concern pattern (`PlaybackEngine.swift` + `+RemoteCommands.swift` + `+NowPlaying.swift`).

### 7.3 State, persistence, SwiftData migration

The current `Persistence.save` rewrites the entire `AppState` JSON to App-Group `UserDefaults` on every mutation. Fine for items / notes / friends / memories; **catastrophic for thousands of transcript chunks each ~250 tokens, plus wiki pages, plus embeddings.** Recommendation:

- **v1: AppState UserDefaults stays.** Items / notes / friends / memories / agent activity log / pending approvals / settings continue to live in `AppState`. Add `subscriptions`, `nowPlaying: NowPlayingSnapshot?`, `briefingScripts` (index only), and new `HomeAction` cases `.openEpisode(UUID)`, `.openPlayer`, `.openBriefing(UUID)`. The `Anchor` discriminated union extends with `.episode(id:)`, `.podcast(id:)`, `.briefing(id:)`, `.transcriptChunk(id:)` cases — this is the bridge that lets existing notes / memories attach to new domain types without changing storage shape.
- **v1.1: SwiftData lands empty.** A `ModelContainer` in the App Group container holds `Subscription`, `Episode`, `EpisodeDownload`, `TranscriptChunk`, `WikiPage`, `BriefingScript`, `EmbeddingRef`. Schema versions explicit; migrations via `SchemaMigrationPlan`. **Items / notes do not migrate** — they remain in `AppState` for backward compat.
- **v1.2+: feature-by-feature entity migration** as new surfaces light up.
- **Vector store** is a separate `vectors.sqlite` (sqlite-vec) file in the App Group container. Keyed only by `episodeID: UUID` — never CloudKit-synced (re-embed from SwiftData transcripts on a new device, or pull cached embeddings from a future server bucket).
- **Widget compatibility.** The widget continues to read a small `NowPlayingSnapshot` struct from App-Group `UserDefaults` via `WidgetPersistence`. We do **not** give the widget a SwiftData container. Pattern is intentionally unchanged.

**Migration sequencing rule (architecture report §12).** Do not ship SwiftData and the agent-prompt rewrite in the same release.

### 7.4 Audio stack — playback + voice + AVAudioSession coordinator

**One `AVAudioSession`, three callers.** `VoiceItemService` (`.record`), player (`.playback + .spokenAudio`), conversation (`.playAndRecord + .voiceChat + .duckOthers + setPrefersEchoCancelledInput(true)`). A single **`AudioSessionCoordinator`** owns every transition or routing breaks.

State machine:
```
A. Idle               .ambient, no active session
B. Playing-only       .playback / .spokenAudio
C. Conversation       .playAndRecord / .voiceChat / AEC on / .duckOthers
D. Briefing+Listening C with the briefing player ducked (-12 to -18 dB)
E. Recording-only     .record (clip extraction, voice-note dictation)
```

Transition `B → C` re-negotiates the route in 50–150 ms; **pre-warm by reactivating with the new category on the wake gesture**, before VAD confirms. After turn, `C → B` ramps the briefing volume back up over 250 ms via `AVAudioPlayerNode.volume`. Briefings are **always paused** on barge-in (not just ducked); answers <8 s duck, ≥8 s pause.

`PlaybackEngine` (AVPlayer wrapper) callbacks fire on a private queue — translate through `MainActor.run { … }` boundaries before touching state.

### 7.5 Transcription pipeline

> Source: [docs/spec/research/transcription-stack.md](research/transcription-stack.md)

**Strategy.** Always check the publisher's `<podcast:transcript>` first (parse VTT / SRT / Podcasting 2.0 JSON into our internal `Transcript` model). When absent, send audio to **ElevenLabs Scribe v1/v2 batch** via async webhook, $0.22/hr — competitive with Deepgram Nova-3 and ~3.3× cheaper than Whisper / GPT-4o-transcribe. iOS 26 `SpeechAnalyzer` is the on-device opt-in privacy mode (no diarization).

**Flow.** RSS poll → new episode → `<podcast:transcript>`? → yes: fetch + parse; no: download audio (Wi-Fi background `URLSession`) → upload to R2 (background `URLSession`, `isDiscretionary = true`) → `POST /v1/speech-to-text` (`model=scribe_v2`, `diarize=true`, `webhook=true`, `cloud_storage_url=…`) → webhook → server-side normalize → device pulls via silent APNs → chunk (400–512 tokens, 15 % overlap, snap to speaker turn within ±20 %) → embed via OpenRouter → INSERT into sqlite-vec — **mark episode "ready for RAG"**.

**Latency.** Plan ~3–6 min end-to-end for a 1-hour episode.

**Cost ceiling.** Power user (50 hrs/wk = ~217 hrs/mo, ~25 % publisher hit-rate) ≈ **$36–50/month**. Typical user (10 hrs/wk) ≈ $7.15/month.

**Internal `Transcript` model is lossless across all source formats** (VTT / SRT / Podcasting 2.0 JSON / Scribe word-list / Apple SpeechAnalyzer / WhisperKit). Adapter pattern: `Transcript.fromScribe(_:)`, `Transcript.fromVTT(_:)`, etc. Each MUST set `source` and `model`, MUST sort segments, MUST stable-id speakers across calls.

**Webhook reliability.** Always have a poll-based reconciliation fallback (`BGAppRefreshTaskRequest`); never trust a single webhook.

### 7.6 Embeddings + RAG (sqlite-vec + OpenRouter)

> Source: [docs/spec/research/embeddings-rag-stack.md](research/embeddings-rag-stack.md)

**Embeddings.** OpenRouter ships an OpenAI-compatible `POST /api/v1/embeddings` endpoint. **Default: `openai/text-embedding-3-large` requested at 1024 dimensions** (Matryoshka truncation, no quality loss). Abstracted behind `EmbeddingProvider` protocol so we can swap to Voyage / Cohere if MTEB benchmarks for our domain demand it.

**Vector store: `sqlite-vec` via `jkrukowski/SQLiteVec`.** Single `vectors.sqlite` in the App Group container. Two virtual tables: `chunks_transcript` (sliding 400–512 tok / 15 % overlap, time-anchored, speaker-tagged) and `chunks_wiki` (~1000 tok semantic chunks, anchored to wiki section headings). FTS5 alongside for BM25.

**RAG pipeline (`query_transcripts`).**
```
agent calls query_transcripts(query, scope?)
  ├─ embed query with text-embedding-3-large @ 1024d   (~150 ms)
  ├─ SQL: vec0 MATCH for top-50 (cosine) WHERE podcastID IN scope
  ├─ SQL: FTS5 BM25 for top-50 WHERE podcastID IN scope
  ├─ RRF merge → top-20                                  (~30 ms)
  ├─ Cohere rerank-v3.5 → top-5                          (~200 ms)
  └─ return [{ episodeID, startSec, endSec, speaker, text, score }]
```
**Total ~400 ms.** For voice mode we drop the reranker (~180 ms total).

**Two indexes, same DB, same schema.** `query_wiki(topic)` hits `chunks_wiki`; `query_transcripts(query, scope?)` hits `chunks_transcript`. The orchestrator can call both in parallel for cross-synthesis. **Combining at query time is wrong** — different chunk sizes confuse RRF; let the agent reason over two retrieved sets.

**SwiftData ↔ vector store integration: keep them separate, bridge by UUID.** SwiftData owns Podcast / Episode / Transcript / WikiPage / prefs / queue / history; `vectors.sqlite` owns chunks (rowid + embedding + FTS) and nothing else. Vectors are **derived data, never CloudKit-synced**.

**Cost.** Power user ingest ~$4.66/year on `text-embedding-3-large`; query+rerank ~$10.4/year. **~$15/year total — not a constraint.**

### 7.7 LLM wiki generation pipeline

> Source: [docs/spec/research/llm-wiki-deep-dive.md](research/llm-wiki-deep-dive.md)

**Generation is automatic** (departing from llm-wiki's user-invoked slash commands):
1. **New episode published** → enqueue transcript fetch → enqueue compile.
2. **Transcript ready** → diarize → chunk → embed → page-update pass on affected entity pages: show page, each speaker's person page, any concept page whose embedding centroid the new chunks fall near.
3. **User listens ≥X %** → optional favorite-quote extraction.
4. **Agent tools** `summarize_episode`, `query_wiki` are read-side, but queries that produce interesting Q→A pairs file back as new pages (Karpathy's "*file valuable explorations back*").

**Per-episode fan-out** replaces llm-wiki's 5/8/10 web-research-agent pattern: `extract_topics`, `extract_entities`, `extract_quotes`, `extract_action_items`, `link_to_existing_pages` — five parallel passes per new episode. Each writes `(episode_id, op)` rows to a queue drained by `WikiCompiler` BGProcessingTask (wakes on charge + Wi-Fi).

**Page taxonomy.** Per-podcast wiki + library-wide hub for cross-show synthesis. Page types: `concept`, `episode`, `show`, `person`, `cross-show debate`. **Confidence is extraction confidence**, not source quality. Provenance-or-it-doesn't-render — every claim points to `(episode_id, start_ms, end_ms)`. The only exception is `Definition` paragraphs with `[general knowledge]` tag.

**Mirror to iCloud Drive Markdown.** Articles persist as Markdown blobs in SQLite (FTS5 + vector index) **and** as files in an iCloud-Drive folder the user can open in Obsidian on Mac. *That single decision is what makes this feel like a personal knowledge base rather than a black-box AI summary.*

**Hallucination defense.** Post-compile verification pass: every synthesized sentence carries a span pointer; cheap classifier or LLM judge checks the cited span actually supports the claim.

**Quote ceiling.** Mirror llm-wiki's <125 char cap on raw quotes for fair-use posture.

**Edit conflicts across devices.** iCloud last-write-wins + monotonic `compile_revision` per page. Not CRDT.

### 7.8 Briefing composition + audio stitching

`BriefingComposer` produces a `BriefingScript` — an ordered list of `Segment` objects, each with: `title`, `tts_body` (TTS narration text), `quotes: [QuoteAnchor]` (original-audio spans to splice), `sources: [(episode_id, start_ms, end_ms, speaker)]`, and `<beat>` markers between sentences for resume-point granularity.

Pipeline:
1. Agent runs `query_transcripts` and `query_wiki` to gather candidates within the requested scope and length.
2. LLM drafts segment titles + bodies; emits source anchors inline; flags `[paraphrased]` if no original audio is available.
3. **TTS render in parallel.** ElevenLabs Multilingual v2 (or v3 GA) streamed and persisted as MP3/Opus per segment, cached in App Group. Stream-as-ready: segment 1 plays before the last finishes.
4. **Stitching.** `BriefingPlayer` chains segments via `AVPlayer` queue with `AVMutableComposition`-style splices for original-audio quotes. Original audio fetch fails → substitute paraphrased TTS, mark chip *paraphrased*.
5. **Branch contract.** Pause-and-resume — main thread freezes at the sample the user spoke over; the branch plays as a parenthetical `Briefing`-shaped sub-object; on completion or *back*, main resumes from that sample. Branches persist and resurface on re-listen.

**Now Playing integration is mandatory.** The briefing is a first-class `MPNowPlayingInfoCenter` episode — lock screen, CarPlay, AirPlay all work without special-casing. Live Activity shows briefing-rendering progress via APNs background pushes (collapse-id `briefing/<id>`).

### 7.9 Agent loop & new tools

**The load-bearing change to `AgentPrompt.swift`: rewrite to *inventory + handles + RAG-via-tools*.** Today it dumps every active item, recent note, memory, and friend into the prompt. That fails at 50 podcasts × 200 episodes. New shape:

```
SYSTEM
  · App identity + persona
  · Subscription inventory: [show_id, title, episode_count, latest_episode_pubdate, last_listened_at]
  · "New this week": last 5 unplayed episodes
  · Current `nowPlaying`: { episode_id, position, transcript_line }
  · Tool descriptions
  · Explicit instruction: "Call search_episodes / query_transcripts / query_wiki to read further. Do not assume content; verify with tools."
TOOLS
  [array — see table below]
USER / ASSISTANT / TOOL turns
```

The agent's **eyes become its tools**; its memory is a vector store. This is the contract the toolset is designed against.

**New tools (added to `Agent/AgentTools+{Podcast,RAG,Wiki,Briefing,Web}.swift`).**

| Tool | Args | Dispatch file | Friend-Reader-tier exposure |
|---|---|---|---|
| `play_episode_at` | `episode_id`, `timestamp_sec` | `+Podcast` | Suggester (draft), Actor |
| `pause_playback` | — | `+Podcast` | Actor |
| `set_playback_rate` | `rate` | `+PodcastActions` | Actor |
| `set_sleep_timer` | `mode`, `minutes?` | `+PodcastActions` | Actor |
| `set_now_playing` | `episode_id`, `timestamp_sec` | `+Podcast` | Actor |
| `search_episodes` | `query`, `scope?` | `+Podcast` | Reader |
| `find_similar_episodes` | `seed_episode_id` | `+Podcast` | Reader |
| `summarize_episode` | `episode_id` | `+Podcast` | Reader |
| `mark_episode_played` | `episode_id` | `+PodcastActions` | Actor |
| `mark_episode_unplayed` | `episode_id` | `+PodcastActions` | Actor |
| `download_episode` | `episode_id` | `+PodcastActions` | Actor |
| `request_transcription` | `episode_id` | `+PodcastActions` | Suggester (draft), Actor |
| `refresh_feed` | `podcast_id` | `+PodcastActions` | Suggester (draft), Actor |
| `open_screen` | `route` | `+Podcast` | Suggester (draft) |
| `query_transcripts` | `query`, `scope?` | `+RAG` | Reader |
| `query_wiki` | `topic` | `+Wiki` | Reader |
| `summarize_speaker` | `speaker_id` | `+Wiki` | Reader |
| `find_contradictions` | `topic`, `scope?` | `+Wiki` | Reader |
| `generate_briefing` | `scope`, `length_min`, `voice?` | `+Briefing` | Suggester (draft), Actor |
| `send_clip` | `clip_id`, `recipient_pubkey` | `+Briefing` | Actor |
| `perplexity_search` | `query` | `+Web` | Reader |
| `delegate` | `recipient`, `prompt` | `+PodcastActions` | Actor, internal only until approval gates land |

Final contract: every mutating tool records an `AgentActivityEntry` with a new `AgentActivityKind` case so per-batch undo keeps working. Current protocol-dependency action tools return JSON envelopes directly; central audit/activity logging should be added in the `ToolGateway` wrapper before remote Actor-tier exposure.

**Reuse of the existing loop.** `AgentChatSession.runAgentTurns` (text) and `AgentRelayBridge` (Nostr inbound, 8-turn cap) both stay. Voice mode (UX-06) hooks `AgentChatSession.send(message, source: .voice)` with a per-sentence callback so TTS streams while the LLM is still generating.

### 7.10 Concurrency, background tasks, lifecycle

**Swift 6 strict concurrency stays on.** Inter-actor boundaries pass `Sendable` value types only.

- **Main-actor:** `AppStateStore`, `AgentChatSession`, `AgentRelayBridge`, `VoiceItemService`, `NostrRelayService`, `ChatHistoryStore`, `AudioConversationManager`, `BriefingPlayer` state, `RAGQueryService` request-coordinator, `PlaybackEngine` observable wrapper.
- **Background:** RSS parsing, OPML parse, transcript chunking, embedding HTTP calls, vector-store reads — `Task.detached` or background actors that return `Sendable` value types and hop back to `@MainActor` for the write.
- **System frameworks:** `AVPlayer` callbacks fire on a private queue; translate through `MainActor.run { … }`. `SFSpeechRecognizer` callbacks already use `MainActor.assumeIsolated` in the existing `VoiceItemService`.
- **Background tasks.** `BGAppRefreshTask` for RSS poll (≤30 s); `BGProcessingTask` for transcription + embedding indexing + wiki compile (longer, deferrable, can require power). Identifiers registered in `Info.plist`.

**Lifecycle additions to `App/Resources/Info.plist`.** `UIBackgroundModes`: `audio`, `fetch`, `processing`. `BGTaskSchedulerPermittedIdentifiers`: feed-refresh ID, transcription ID, embedding-index ID, wiki-compile ID. `NSAppleMusicUsageDescription` (lock-screen now-playing). `NSMicrophoneUsageDescription` and `NSSpeechRecognitionUsageDescription` (voice mode). `NSUserActivityTypes` already updated for the renamed scheme.

**Entitlements.** `Podcastr.entitlements` already in place. CarPlay (`com.apple.developer.carplay-audio`) added in v1.1.

### 7.11 Settings, secrets, Keychain

Existing Keychain stores follow a uniform pattern: `(service, account)` with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, all access through a typed enum. New entries follow the same shape:

- `PerplexityCredentialStore` — for `perplexity_search`.
- `EmbeddingProviderCredentialStore` — only required if OpenRouter cannot serve the embedding model we choose. **Verify before wiring** — OpenRouter's embedding coverage has historically been thinner than its chat coverage. If we end up calling Voyage / Cohere / OpenAI directly, this store handles their key.

**Settings extensions** (non-secret; `iCloudSettingsSync` already merges arbitrary fields key-by-key, so cross-device sync comes free): `transcriptionPreference: { publisher | scribe | local }`, `embeddingProvider`, `embeddingModel`, `briefingVoiceID` (reuse `elevenLabsVoiceID`), `briefingDefaultLengthMin`, `downloadOverCellular`, `defaultPlaybackRate`, `skipBackSeconds`, `skipForwardSeconds`, `dailyBriefingTime`, `dailyBriefingWeekendTime`, `pushBudget`, `incognitoSearch`, `hideTranscriptOnLockScreen`, `voicePersona`.

---
