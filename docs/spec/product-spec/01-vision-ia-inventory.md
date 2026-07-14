# Product Spec: Vision, IA, and Inventory

> Part of the Podcastr product spec. Start at [PRODUCT_SPEC.md](../PRODUCT_SPEC.md).

## 1. Vision & North Star

> Source: [docs/spec/PROJECT_CONTEXT.md](PROJECT_CONTEXT.md), [docs/spec/briefs/ux-15-liquid-glass-system.md](briefs/ux-15-liquid-glass-system.md)

Podcastr is a next-generation iOS podcast player built around an embedded AI agent that has **perfect knowledge of every podcast the user is subscribed to** — including the episodes they have not listened to yet. The user can talk to all of their podcasts as if they were one continuous conversation, by text or by voice, and the agent answers grounded in transcripts, wikis, and the open web — with citations that are tap-to-play.

The product makes three promises that no other podcast app currently makes together:

1. **The library is searchable as language, not just titles.** Every episode is transcribed, diarized, chunked, embedded, and indexed locally. Vague half-memories (*"that one about stamps"*) become tap-to-play moments.
2. **The agent produces, not just retrieves.** It synthesizes a personalized morning briefing — an entire NPR-quality episode written and narrated for the user, stitched with original quotes — and is **interruptible mid-narration**: the user speaks, the briefing ducks, the agent answers, the briefing resumes from the same word it dropped.
3. **The bar for craft is editorial.** iOS 26 Liquid Glass everywhere; New York serif for content; SF Pro for chrome; cinematic motion that *means something*; haptics and sound design that earn their place. Calm by default, alive on demand. Better than Spotify, Overcast, Pocket Casts, Castro on the floor — and an agent layer no one else can match on top.

**Design north star (single sentence):** *the home screen feels quiet; the player breathes; the agent is electric — and the user can always feel which is which.*

**Foundations we inherit (do not rebuild):** SwiftUI + Tuist, iOS 26 deployment target, Swift 6 strict concurrency, an `@Observable` `AppStateStore`, a working tool-calling agent loop over OpenRouter SSE, a complete Nostr subsystem (relays, keypair, ACLs, agent relay bridge for cross-device DMs), Keychain BYOK stores for OpenRouter and ElevenLabs, shake-to-feedback, Live Activities and widgets via the App Group, App Intents + Siri, GitHub Actions → TestFlight CI. The skeleton is renamed end-to-end to **Podcastr**, builds green, and has stubbed module/feature folders waiting for this spec.

---

## 2. Marquee User Stories

These five moments are the demo reel and the acceptance test for v1. If any one of them does not land, v1 is not done.

| # | Story | Demonstrates |
|---|---|---|
| 1 | *"Play the part of yesterday's Tim Ferriss where he talked about keto."* — agent finds the timestamp, opens the player, presses play. <2 s perceived. | Voice + RAG + `play_episode_at` tool + transcript ingest pipeline working end-to-end. |
| 2 | *"What was that podcast last week about stamps?"* — fuzzy semantic recall surfaces a clip card; tap plays at the right second. | Embeddings + hybrid FTS5/vec search + clip card surface. |
| 3 | *"Give me a TLDR of this week's podcasts in 12 minutes."* — `generate_briefing` produces a TTS-narrated synthesized episode in <90 s, plays it, the user says *"Wait, who was that guest?"* mid-briefing — the briefing ducks, the agent answers from RAG, the briefing resumes at the same syllable. | Briefing composer + barge-in voice mode + audio session coordination + LLM streaming. |
| 4 | *"Send my partner a clip of the part where she's mentioned."* — agent finds the speaker name + timestamp, builds an audio + waveform clip card, shares via Nostr DM (or iMessage) with provenance. | Diarization + clip composer + Nostr send pipeline. |
| 5 | *"What does this podcast say about Ozempic across all their episodes?"* — agent runs cross-episode synthesis from transcripts + wiki, returns a comparison block with two-column pull quotes and contradictions surfaced. | LLM wiki + cross-episode threading + hallucination guardrails (every claim cites a span). |

A sixth story, *"What's a contrarian take on what they just said?"*, exercises `perplexity_search` and is included as the differentiator versus closed-corpus assistants. It ships at v1 if the BYOK path for Perplexity lands; otherwise v1.1.

A seventh story is **in-episode voice drop**: while the episode is playing, the user taps the agent chip and speaks a one-sentence thought. The agent has full context of the current transcript window (what was just said) and the user's speech, then acts: *"rewind to where this topic started"* → seeks to the transcript anchor; *"clip that"* → builds a waveform clip card at semantically correct in/out points; *"I wonder how this applies to X"* → drops a timestamped note and optionally fires off a research thread. The user never leaves the Now Playing screen. See [docs/spec/briefs/ux-16-in-episode-agent.md](briefs/ux-16-in-episode-agent.md).

---

## 3. Information Architecture & Tab Structure (resolved)

> Source: [docs/spec/research/skeleton-bootstrap-report.md](research/skeleton-bootstrap-report.md), [docs/spec/briefs/ux-02-library.md](briefs/ux-02-library.md), [docs/spec/briefs/ux-04-llm-wiki.md](briefs/ux-04-llm-wiki.md), [docs/spec/briefs/ux-14-proactive-agent-notifications.md](briefs/ux-14-proactive-agent-notifications.md)

UX-02 proposed three tabs (Library / Listen / Agent). The skeleton already shipped six (Today, Library, Wiki, Ask, Home, Settings) and UX-04, UX-08, UX-09, UX-13, UX-14 all implicitly assume Today and Wiki as destinations. **Resolution: skeleton wins**, with template Home retired before TestFlight.

### Final tab structure (v1)

```
┌─ Tabs ─────────────────────────────────────────────────────┐
│  Today    Library    Wiki    Ask    Settings              │
└────────────────────────────────────────────────────────────┘
   editorial subscriptions  knowledge  agent + global search   prefs
   "front       + downloads + people   + voice mode entry
    page"        + Discover  + topics
```

- **Today** — the proactive surface (UX-14): one daily briefing card, three to five insight cards (Drops / Threads / Echoes / Friends / Transcript-ready). Editorial cover, finishable in under a minute. Pull-down reveals yesterday's edition (7-day window). Empty days show a *"Quiet day."* cover, not a feed.
- **Library** — subscriptions, downloads, transcription queue, OPML, smart playlists (UX-02). Includes a **Discover** segmented sub-tab for agent-curated recommendation rails (offline-generated; *pull*, not *push*).
- **Wiki** — the LLM-compiled knowledge base (UX-04). Library-wide hub at the root; per-podcast wikis as a chip filter, *not* a separate destination. Topic, person, debate, graph (one level deep, opt-in), and Generate-Page surfaces.
- **Ask** — the agent's home (UX-05 + UX-07). Persistent search field at the top doubles as the chat composer; semantic search results stream beneath, with a single *"Ask the agent instead →"* card surfaced when the query reads as synthesis. Voice mode (UX-06) is reached from the mic button on this tab and from the Action Button / AirPods / Lock Screen.
- **Settings** — preserved verbatim from the template; extended with Voice, Briefing, Notifications, Friends, Identity, Relays, BYOK keys.

### Surfaces reached via navigation, not via the tab bar

- **Now Playing** (UX-01) — persistent mini-bar across all tabs; expands to full-screen via `matchedGeometryEffect`.
- **Episode Detail / Transcript Reader** (UX-03) — pushed from any subscription, search result, agent citation, or Today drop card.
- **Voice Mode** (UX-06) — modal full-screen orb; entered from Ask mic, Action Button, AirPods long-press, Lock Screen control, Siri intent, CarPlay voice button.
- **Briefing Player** (UX-08) — modal; entered from Today hero, Ask briefing card, Library briefings shelf, push notification.
- **Speaker / Topic Profile** (UX-13) — pushed onto the navigation stack of whichever tab summoned it (most often Wiki or transcript long-press).
- **Threading sheet** (UX-09) — bottom sheet at 40 % / 90 % detents; never a destination page.
- **Agent Chat thread list** (UX-05) — a sheet from the Ask tab on iPhone; a sidebar on iPad.

### Boundaries between agent surfaces (this is the contract)

- **Search** shows you the evidence (clips, episodes, transcript moments). It never argues or summarizes; the user picks.
- **Ask Agent** synthesizes prose answers grounded in tools. Editorial serif, unbubbled.
- **Briefings** produce *audio episodes* as artifacts. Agent owns this stage.
- **Wiki** is the long-term, compiled, human-readable knowledge base. Paper-feel; glass is chrome only.
- **Threading** is the contextual whisper across surfaces — never a tab.
- **Today** is the front page; one push a day, max. Surplus pools here.

---

## 4. Feature Inventory

### 4.1 Baseline / Table-Stakes

> Source: [docs/spec/baseline-podcast-features.md](baseline-podcast-features.md)

We commit to category parity, not victory, on the floor. The agent layer is the differentiator. Missing any **must** row at launch is a category-disqualifier.

| Area | v1 must-haves | v1.1 fast-follow | v2 |
|---|---|---|---|
| Playback | Variable speed (0.5×-3×, 0.05× steps, per-show + global default), Smart Speed silence trim, Voice Boost, asymmetric skip (15 / 30 default), sleep timer (incl. end-of-episode / chapter), chapter rendering (ID3, MP4, Podcasting 2.0), AirPlay 2, Bluetooth controls, full Now Playing + Remote Command Center, smart Up Next queue, resume position, mark-played thresholds, bookmarks, AI-generated chapters when none in feed, background audio, interruption handling | Volume normalization, long-press scrub, shake-to-extend sleep timer, Handoff iPhone↔iPad↔Mac↔Watch, episode-update detection, clip creation + share, premium / private feeds (Patreon / Supercast tokens) | Smart playlists, alt-feeds, Apple Podcasts Subscriptions OAuth, Spatial Audio, SharePlay co-listen |
| Subscriptions | RSS URL add, OPML import + export, iTunes Search API directory, Podcast Index integration, manual + scheduled (`BGAppRefreshTask` ~1 h Wi-Fi) refresh | Podcasting 2.0 namespace (chapters / transcripts / persons / soundbite / location / value), premium / private feeds, episode-update detection | — |
| Episode mgmt | Manual download + retry, auto-download policy (off / latest N / all new, Wi-Fi-only), storage UI, auto-delete after played, filter views (Unplayed / In Progress / Downloaded / Starred / Archived), sort, bulk mark-played, played-state viz, star / favorite | Archive, custom playlists, episode size cap, queue / history sync | Smart playlists, translated transcripts |
| Sync | iCloud (CloudKit) for subs + position + played-state, last-writer-wins per field with vector clock for position | Queue, badge, history sync, RTL + CJK + initial localizations (es / pt / ja / de / fr) | — |
| A11y | Dynamic Type to AX5, full VoiceOver coverage, high contrast, reduce motion, reduce transparency, captions / transcripts | RTL layouts, CJK font fallback, language indicator | Translated transcripts, on-device translation |
| Notifications | New-episode push, per-show toggle, agent / proactive notifications scaffolding | Quiet hours, download-complete (off by default) | — |
| Search | In-library keyword, in-show, Apple Podcasts directory, Podcast Index, semantic / agent (UX-07) | Recent / saved searches | — |
| Sharing / social | Share-with-timestamp universal link, App Intents + Siri, Live Activity (Now Playing + Dynamic Island), small + medium widgets | Clip share (audio + waveform card, video w/ subtitle burn-in), large widgets, action button shortcut presets | SharePlay co-listen, V4V Lightning tipping, boostagrams |
| Privacy | Analytics opt-out (default), data delete, per-show cache clear, honest privacy nutrition, ATT default-no | Data export (OPML + JSON), diagnostics export | — |
| Settings | Theme + accent, default speed + skip, auto-download defaults, restore from iCloud, sleep-timer defaults | Reset all, diagnostics export | — |
| Platform | Apple Watch standalone playback + downloads, iPad Stage Manager, PiP, video-podcast playback | Mac Catalyst, external display | Spatial Audio, Family Sharing |

The brief at `docs/spec/baseline-podcast-features.md` governs the *requirement to exist* for everything in this table; the surface briefs (UX-01-15) govern the *experience*.

### 4.2 Differentiating (Agent + Knowledge)

These are what no competitor has stitched together. Each row must work in concert with the surface owning it.

| Capability | Owning surface | New tools / engines |
|---|---|---|
| Timestamped diarized transcripts on every episode | UX-03, UX-01 | `Transcript/` module, `ScribeTranscriptionClient`, `PublisherTranscriptFetcher`, `TranscriptChunker` |
| LLM-compiled per-podcast and library-wide wikis | UX-04 | `Knowledge/WikiPage`, `WikiCompiler` (BG task), agent tool `query_wiki` |
| Hybrid lexical + vector RAG over transcripts | UX-05, UX-07 | `Knowledge/VectorIndex` (sqlite-vec), `RAGQueryService`, agent tool `query_transcripts` |
| Voice conversational mode with sub-second barge-in | UX-06 | `Voice/AudioConversationManager`, `BargeInDetector`, ElevenLabs Flash v2.5 streamer |
| Personalized audio briefings (interruptible) | UX-08 | `Briefing/BriefingComposer`, `BriefingPlayer`, agent tool `generate_briefing` |
| Cross-episode threading (timeline / contradictions / evolution) | UX-09 | Threading inference job (BG), agent tools `find_contradictions`, `find_similar_episodes` |
| Speaker + topic profiles | UX-13 | Speaker resolver (RSS notes → NER → voiceprint), agent tool `summarize_speaker` |
| Snipd-parity learning loop: headphone / CarPlay snips, mentioned books, guest graph, auto-chapters, AI DJ-style routes | UX-01, UX-03, UX-04, UX-07, UX-11, UX-13 | Span-grounded `Snip` model, entity extraction workers, `Book` resolver, `Person` resolver, `ChapterCompiler`, `PlaybackRouteCompiler`; see [Snipd Feature Model](research/snipd-feature-model.md) |
| Proactive editorial Today | UX-14 | Insight ranking job (BG), `IsightCard` taxonomy, push budget (1/day default) |
| Nostr-mediated cross-device + friend agent | UX-12 | Existing `NostrRelayService` + `AgentRelayBridge`, new `PermissionTier`, `toolOverrides` on `Friend` |
| Onboarding to first briefing in <90 s | UX-10 | Trial budget service, OPML detection animation |
| In-episode voice drop — context-aware agent actions while listening | UX-16 | `InEpisodeAgentController`, `TranscriptWindowProvider`, agent tools `seek_to_topic_start`, `create_clip_semantic`, `anchor_note`, `research_inline` |

---
