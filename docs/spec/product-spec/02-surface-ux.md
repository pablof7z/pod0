# Product Spec: Surface UX

> Part of the Podcastr product spec. Start at [PRODUCT_SPEC.md](../PRODUCT_SPEC.md).

## 5. Surface-by-Surface UX

> Each subsection is ≤500 words and links to the source brief. The brief stays canonical for full microinteraction tables, ASCII wireframes, and prose. This document captures **decisions, contracts, and handoffs**.

### 5.1 Now Playing

> Source: [docs/spec/briefs/ux-01-now-playing.md](briefs/ux-01-now-playing.md)

The hero surface and the user's most-stared-at screen. The transcript is the primary surface; the artwork is the frame. As audio plays, the active sentence rises and lights up, tinted by the speaker's diarized color (extracted from cover art for cross-show memorability). Every line is a doorway: tap to jump, hold to clip, double-tap to ask the agent. The waveform under the scrubber shows the *shape* of the conversation — speaker stripes, ad-break bands, silence — not just amplitude.

**Three modes redistribute weight:** artwork-dominant (default, 42 % artwork / 30 % transcript), transcript-focus (12 % / 60 %, entered by swipe-up), and scrub-mode (waveform expands to 220 pt with speaker stripes; transcript dims to 30 %).

**Three glass tiers, exclusive copper accent.** Hero glass for the transcript card, control glass for the transport row (one `GlassEffectContainer` so play / skip morph as one liquid bead), and a *single* tinted agent chip in the lower right — the only thing on screen that should glow. Copper (`accentPlayer`) appears here and nowhere else outside the player chrome.

**Mini-bar signature.** The persistent mini-bar across tabs uniquely shows the **active transcript line**, not just the title — a 1-line ticker. This is the visual signature that signals "this app understands the audio."

**Handoff contract.** Tap show name → Episode Detail (UX-03). Long-press transcript line → inline agent answer (≤3 turns); fourth turn promotes to Agent Chat (UX-05). Double-tap a noun the agent has linked → Wiki peek sheet (UX-04). Voice button on the agent chip → Voice Mode (UX-06). **Tap the agent chip while an episode plays → In-Episode Agent (UX-16)**: episode ducks, orb rises, user speaks a thought, agent acts (seek / clip / note / research) without leaving the player. Queue chip → Queue sheet (UX-02 owns).

**Microinteraction discipline.** Hold-to-clip is 600 ms with rising haptics (`.light` → `.medium` → `.heavy`); release before commit cancels. Scrub release snaps to the nearest sentence boundary within ±400 ms. The "Return to live" pill (Slack pattern) appears when the user manually scrolls; auto-scroll resumes only on tap. Speed dial appears on long-press of the play button (280 ms hold) — one-thumb operable, no menu dive.

**Edge cases.** Streaming transcript (live Scribe ingest): un-arrived region renders as faint blur + caret; tapping ahead shows *"Transcribing… tap to jump anyway."* Uncertain diarization: speaker chip reads "Speaker 2" with a dotted outline plus a long-press *Name this speaker* sheet. We never silently hide ambiguity. CarPlay handoff: we publish chapter, speaker, and current line through `MPNowPlayingInfoCenter` so CarPlay's surface can render speaker chip + active line.

**Performance budget.** 120 Hz on iPhone 15 Pro+, graceful degrade to 60 Hz on iPhone 13. Live glass + auto-scrolling transcript + waveform redraw is the most demanding surface in the app.

### 5.2 Library & Subscriptions

> Source: [docs/spec/briefs/ux-02-library.md](briefs/ux-02-library.md)

Library is the **calm room** of the app — editorial, still, generous. The agent surfaces are alive; this one is not. A user opens Library to *orient*, not *interrogate*. Querying belongs to Ask (UX-05/07); chatting belongs to Ask too; this surface answers one question with care: *what's here, and what's ready for me?*

**Layout.** Continue Listening rail (≤3 cards with progress arcs) at the top; segmented control [Subscriptions | Downloads | Discover]; below that, view toggle (grid 3-up iPhone / 4-up iPad, or dense list), sort, and filter chips (All / Unplayed / Downloaded / Has transcripts). Smart Playlists collapsed below. Show detail uses a 220 pt artwork hero with extracted-tint gradient bleed.

**Glass discipline.** Liquid Glass is **structural only** — tab bar, mini-player chrome, sticky filter bar, OPML sheet, transcription progress capsule. Cards and rows are matte so artwork breathes. This is the only major surface where T0 (paper) dominates over T1/T2 (glass).

**Discover sub-tab.** Agent-curated recommendation rails (*Because you finished X*, *New from voices you trust*, *One short episode for your commute*) — generated *offline*, not in real time. *Why this?* tap reveals one paragraph of agent rationale with sources. **Discovery is *pull*; Today (UX-14) is *push*.** Same agent, different surface.

**OPML import flow.** Drag-drop or file picker → parses → preview checklist (deselect any) → import. Mass-import shows a real progress bar plus per-show transcription queue indicator and is fully backgroundable via Live Activity. Malformed OPML: first 5 parse errors with line numbers, then *"Import what we could parse (212 of 247)."*

**Transcription status capsule.** Per-row: `Downloaded · Transcribing 64 %` or `Ready` or `Queue #3`. Tappable; expands inline to a 3-row queue preview with cancel-per-item. This is the surface where the transcription pipeline becomes legible.

**Local filter ≠ semantic search.** Library's filter chips operate on structured fields (status, duration, show, date, transcription state). Semantic search lives in Ask. Smart Playlists are local-structured; agent-generated playlists live in Discover with provenance.

**Handoff contract.** Subscription → show detail (Library-owned). Show detail → Episode Detail (UX-03). Episode Detail → Now Playing (UX-01). Discover card → Episode Detail or *Why this?* expansion (Library-owned).

**Edge cases.** Empty library: editorial hero + three stacked affordances (Import OPML / Paste RSS / Browse Discover); the agent does **not** speak here. Offline: subscriptions render from cache with an *Offline* glass chip top-right; downloads remain fully functional. Transcription quota exhausted: non-modal banner *"Transcription paused: monthly quota reached."* Actions: *Use on-device* / *Upgrade*; never blocks playback. Subscription removed by feed: show dimmed with `Feed gone` label; episodes still listenable; *Find replacement* hands off to UX-07.

### 5.3 Episode Detail & Transcript Reader

> Source: [docs/spec/briefs/ux-03-episode-detail.md](briefs/ux-03-episode-detail.md)

Where a podcast becomes a **document**. Three modes share the same content:

- **Episode Detail** — hero (artwork, show, guest, date, duration) + show-notes HTML + chapters list + agent-generated lede summary + "Open transcript" CTA + related-episodes rail. Floating glass mini-player.
- **Reading Mode** — player vanishes; single column; editorial type; subtle progress gutter on the leading edge; pure prose. The ad-free, listening-free reading room.
- **Follow-Along** — audio playing **and** transcript visible. Current sentence tinted; page auto-scrolls to keep it in the upper third (Kindle pattern). Vertical chapter rail on the trailing edge as a Liquid Glass strip with `glassEffectID` morphing to track scroll position. Tap any sentence to scrub audio there.

**Typography is the load-bearing decision.** Body is **New York** (Apple's editorial serif, optical sizing, Dynamic Type-aware). UI chrome and timestamps are SF Pro Text. Speakers are SF Rounded Semibold. The serif is what signals *document, not interface.* Body 19/30 (1.58 leading); 64-character optimum clamp engages from iPad width upward.

**Long-press is the primary interaction.** 300 ms long-press on a sentence lifts it with a 1.02× glass scale and soft haptic; selection handles snap to sentence boundaries; drag to expand to paragraph or contract to phrase. Action bar: *Copy · Share Image · Clip · Ask Agent · Bookmark*. Two-finger vertical drag = span-clip selection with waveform preview.

**Annotation taxonomy.** Highlights = soft yellow tint on the sentence's text background (never on the glass chrome). Notes = sentence-attached, surface as a tiny glass asterisk in the leading margin. Bookmarks = solid dots on the chapter rail at exact positions. All three roll up into a per-episode *Marks* tab and a global view, reachable via VoiceOver rotor.

**RAG citations.** Not chips (too noisy). 2 pt dotted underline + superscript glass dot. Tap → glass popover (*"Contradicted in Episode 142"*). Hand-off to UX-09 threading.

**Wiki links.** Solid 1 pt underline in `tintColor.secondary`. Tap pushes UX-04; long-press peeks a glass card.

**Live transcript UX.** Streaming Scribe ingest: partial transcript renders progressively, animated skeleton paragraphs below the cursor. Reads as `Transcribing… 38 %`; *Notify me when ready* CTA. ETA shown in caption; we never lie about the match.

**Quote-share card.** Generated image with artwork, speaker, timestamp, and a deep-link back. Three output formats: image, audio + subtitle-burned video, link.

**Clip composer.** Drag handles; sentence-snapped (word-snap via second long-press). Subtitle style row: *Editorial / Bold*, speaker-labels toggle. Output to clip-share targets (universal link, iMessage, Twitter/X, Mastodon, Nostr, copy audio).

**Edge cases.** No transcript / no budget: show notes become the readable object; one-tap *"Transcribe this episode (1 credit)."* Low-confidence regions (<0.6 confidence): 1 pt dotted underline; long-press shows top-3 alternates + *report correction*. 5 h Lex Fridman: virtualize the transcript (±2 chapters around scroll laid out); long-press the chapter rail expands a full-height speaker timeline minimap. Live episode: hide *Read transcript* until first chapter finalized; show *Transcribing live* badge.

### 5.4 LLM Wiki Browser

> Source: [docs/spec/briefs/ux-04-llm-wiki.md](briefs/ux-04-llm-wiki.md), [docs/spec/research/llm-wiki-deep-dive.md](research/llm-wiki-deep-dive.md)

The **second brain of the listener's library**. Britannica energy. Whole Earth Catalog density. Tufte respect for evidence. Stripe Press typography. Where the player is hot, the wiki is cool — a quiet, beautifully set reference work the user retreats into to *understand* what they have heard.

**Adapted from nvk/llm-wiki, with three departures.** (1) Generation is **automatic**, not user-invoked: new episode → transcript fetch → diarize → chunk → embed → page-update pass. (2) Citations are first-class — every claim points to `(episode_id, start_ms, end_ms)`, not a URL. Each rendered sentence has a tappable timestamp chip that calls `play_episode_at`. (3) The page taxonomy adds a **library-wide hub** for cross-show synthesis (the killer "*what does the podcast world say about Ozempic?*" feature) on top of per-podcast wikis.

**Confidence semantics shift from llm-wiki's "source quality" to our "extraction confidence"** — transcript right, diarization right, faithful synthesis. Cite the span; let the user tap-to-verify. Single-source claims render with an amber dotted left rule and the inline tag `(1 source)`. Zero-corroboration claims get `(uncorroborated)`. **Provenance-or-it-doesn't-render** — the only exception is *Definition* paragraphs, which may carry `[general knowledge]`.

**The page is paper, not glass.** This is the only major surface where Liquid Glass is reserved for *floating* elements only — the citation peek sheet, the wiki bar, the time-slider, the Generate handle, the contradiction popover. All glass is `.regular.interactive()` in `.rect(cornerRadius: 22)`. Body is New York serif at 17/26, generous measure (~62 ch on phone, two-column at 78 ch + 22 ch margin on iPad).

**Topic Page is the heart.** Definition (1 paragraph, italic, evidence-graded) → Who's discussed it (avatar row, links to UX-13) → Evolution timeline (compact horizontal strip; scrub renders the page *as it would have read* on that date) → Consensus vs Contradictions (split column) → Related (typed chips) → Citations (every episode + timestamp, sortable).

**Citation peek.** Hover/long-press a citation chip → glass sheet rises 1/3 from bottom, autoplays the cited 12 seconds, dismisses on release. Pulling up expands to full transcript at that timestamp (hands to UX-03).

**Generate Page flow.** Search bar that finds nothing reveals *"Compile this →"* glass handle. Pulling it down progressively unlocks the page — paragraph blocks slide up one at a time as the agent finishes each, citations resolving from `[…]` placeholders to amber pips. Expensive per regen — aggressive claim-level caching is required (only re-synthesize claims whose underlying transcripts changed).

**Graph view.** Burying it one level deep ("Wiki home → Graph") is deliberate. The graph is dazzling in demos and shallow in daily use. Reject the temptation to make it the home.

**Wrong-button protocol.** Long-press any sentence → contextual menu (*Cite · Compare · This is wrong · Quote*). *This is wrong* opens a 5-second voice-memo glass card; submission marks the claim as `contested` until next regeneration; the next regen must remove, downgrade, or surface a counter-citation.

**Boundary with UX-09 threading.** Threading is **episodic recall** ("here's every time keto came up"); wiki is **synthesized knowledge** ("what does keto mean in your library?"). Every threading detail sheet has a footer button: *Open the wiki entry →*. Single hand-off, never duplicated content.

**Boundary with UX-13 profiles.** Wiki person cards show only avatar, role, top-3 claims, *Open full profile →*. No bio prose. UX-13 owns the depth.

### 5.5 Agent Chat (Text)

> Source: [docs/spec/briefs/ux-05-agent-chat.md](briefs/ux-05-agent-chat.md)

iMessage's composure crossed with a magazine's voice. Not a chatbot UI — a *reading and listening surface* where you talk to every podcast you've ever subscribed to, and it talks back in artifacts: episode cards that play in place, transcript pull-quotes set in editorial type, wiki peeks, citations that hover like footnotes.

**Three principles.** (1) The answer is the hero, the mechanism is the footnote — tool calls collapse by default. (2) The chat knows where the playhead is — composer carries an ambient *Now Playing* chip the user can tap-attach. (3) Threads are conversations *with podcasts*, not sessions *with a bot* — each subscribed show gets an evergreen thread; cross-podcast questions live in *General*; briefings get their own threads.

**The signature decision: bubbles vs. unbubbled editorial.** User messages live in subtly tinted glass capsules, right-aligned. **Agent messages are unbubbled** — set in editorial serif, left-aligned, generous leading, like a column of *The Atlantic*. This separates us from every other chat app on iOS.

**Embedded media as Liquid Glass cards.** Six card shapes share a common shell (`GlassEffectContainer` spacing 32, cornerRadius 22): episode card (56 pt artwork + show/episode/progress/play glyph), clip card (waveform + scrubber + in/out timestamps), wiki peek (eyebrow + 2-line excerpt + chevron), transcript excerpt (small-caps speaker + italic body + glass leading rule), Perplexity citation (numbered footnote chip + sources row of glass pills), and comparison block (two-column pull-quotes for cross-podcast disagreement).

**Action-chip rail** sits sticky just above composer, contextual to the last agent reply. Max 4 visible, horizontal scroll for more. Primary action is `.glassProminent`; rest are `.glass`.

**Tool-Call Inspector** is the trust surface. Collapsed badge by default (*"▾ used 3 tools"*); two-finger tap on agent message instantly opens it; first-run coachmark explains it. Inspector lists each tool with timing, args (pretty-printed), and *Show evidence* link to the exact transcript chunks used.

**Now-Playing chip in composer.** Default-attached when audio is active; one tap to remove, never silent. Tap-and-hold attaches a *range* (use scrubber to pick).

**Drag-from-Library.** Episode dragged from UX-02 lands as an attachment chip in the composer; user types the question; agent receives episode as grounding context.

**Per-podcast thread proliferation policy.** Lazy-create on first interaction — a user with 60 subs does not start with 60 empty threads. Global search across threads in the thread list.

**Edge cases.** Agent thinking >2 s: replace typing dots with a *progress chip* showing the active tool name (*"searching transcripts… 1.2 s"*). >8 s: append a cancel affordance. Tool failure: inline glass *whisper* in muted red ("Couldn't reach the wiki for *Zone 2* — retrying in 5 s. [Retry now]"); failed tool stays in inspector with a red dot. Rate-limited: top-of-thread banner ("You've used 80 % of today's deep searches. Lighter answers below."), tone matter-of-fact. No internet: composer disables online tools; chip *Offline · using on-device transcripts only*. Streaming interrupted mid-tool: partial answer remains with a *Resume* pill.

**Friend agent DMs (UX-12) live in the same thread list** with a small relay glyph; message-stream UI is identical so users don't have to learn two surfaces.

### 5.6 Voice Conversational Mode

> Source: [docs/spec/briefs/ux-06-voice-mode.md](briefs/ux-06-voice-mode.md), [docs/spec/research/voice-stt-tts-stack.md](research/voice-stt-tts-stack.md)

The feature that should make a stranger gasp on first use. The marquee moment is **barge-in mid-briefing**: the agent is reading a 12-min TLDR, the user says *"Wait — who was that guest?"*, and within ~120 ms three things happen: the agent's voice ducks and dissolves mid-syllable; its glass orb inhales (collapses from speaking bloom to listening lens, pulses with the user's voice); the briefing's now-playing card dims and recedes one z-layer (*"holding your place"*). When the answer is delivered, the orb exhales and the briefing card swims back forward.

**Three principles.** The agent is a guest in your ear — it defers, always. Glass breathes — the orb's state is legible without text. Audio first, screen optional — every voice interaction works blind, one-handed, in a car, with AirPods only.

**Two voice modes, one visual language.** Push-to-talk (PTT) listens *only* during gesture / hot-phrase window. **Ambient (barge-in) listens automatically only while the agent itself holds the audio session** — never an always-on home-screen mic. This is the privacy contract.

**Single state machine drives the orb:** idle → listening → transcribing → thinking (with tool chip morphed from orb perimeter via `glassEffectUnion`) → speaking (96 pt bloom with 2.4 s breath rhythm) → bargeIn (snap to 0.7×, listening-blue tint, halt breath) → listening / idle.

**Latency budget (target 800 ms first audio frame, ceiling 2 s).** End-of-speech VAD ~250 ms + final STT commit 50–150 ms + LLM first token 300–700 ms (prompt-cached) + TTS time-to-first-byte 75–150 ms (ElevenLabs Flash v2.5 WebSocket) + network jitter 50–150 ms. To earn this: stream STT partials into LLM speculatively; pipe LLM response to TTS per sentence; pre-warm TTS WebSocket on VAD start; cache OpenRouter system prompt + tool schema (~200–400 ms saved per repeat turn).

**Stack.** Live STT: **Apple `SpeechAnalyzer` (iOS 26+)** — sub-second partials, ~2× Whisper Turbo, fully on-device. `SFSpeechRecognizer` for iOS-25-and-below fallback (we target iOS 26 so this is rarely hit). WhisperKit only behind a Pro toggle; cloud STT (Scribe) is **for offline transcript ingest only**, never the live loop. Conversational TTS: ElevenLabs Flash v2.5 streaming. Briefing TTS: ElevenLabs Multilingual v2 (or v3 GA) pre-rendered, played via `AVPlayer`. Fallback: `AVSpeechSynthesizer` Premium / Personal Voice for offline / privacy mode.

**Audio-session contract — the load-bearing decision.** State A (briefing only): `.playback + .spokenAudio`. State B (conversation active): `.playAndRecord + .voiceChat + setPrefersEchoCancelledInput(true) + .duckOthers`. **Never `.mixWithOthers`** — disables AEC. Switch is centralized in a single `AudioSessionCoordinator`; pre-warm on the wake gesture before VAD confirms speech.

**Barge-in detection.** AEC always on (~95 % of speaker leak gone). Apple's `SpeechDetector` (iOS 26) co-optimized with `SpeechTranscriber`, or Silero VAD via ONNX as fallback. Cross-correlate against a 500 ms ring buffer of TTS output to eliminate the agent's own voice. Require ~250 ms voiced audio before firing — slower than Alexa (100 ms) but eliminates iPhone-speaker false positives. **Optimistic preview**: orb's edge gets a faint rim-light the instant VAD triggers, *before* STT confirms. False-positive (cough) → rim fades, TTS resumes, no perceived stutter. This is the secret to the magical feel.

**Triggers.** A single `AppIntent` (`StartVoiceModeIntent`) covers Action Button, AirPods squeeze (long-press, user-assigned in Settings → AirPods → Press and Hold), Lock Screen control (`ControlWidget`), Siri ("Hey Siri, ask Podcastr…"), Spotlight, and CarPlay button. Wake-word ("Hey, podcast") deferred to v1.1 — invites parody and false triggers in podcast audio itself.

**Privacy.** Mic prompt fires on first voice-mode invocation, never at launch. On-device STT/TTS by default; one Settings toggle opts into cloud voices for higher quality. Captions always available. Voice queries processed on-device; only final text persists to chat history; audio buffers never written to disk. *Delete voice history* action in Settings.

### 5.7 Semantic Search & Discovery

> Source: [docs/spec/briefs/ux-07-search-discovery.md](briefs/ux-07-search-discovery.md)

**Search is not the agent.** The agent argues, summarizes, generates briefings. Search **shows you the evidence** and gets out of the way. The user often does not want a synthesis; they want the receipt.

The search bar lives at the top of the **Ask** tab — always-on, never collapsed, never hidden behind a magnifying glass. It is a destination, not a utility. It doubles as the agent composer: when the query reads as synthesis (starts with *summarize, compare, explain, what did, why does*), a single card appears at the top of the results: *"Ask the agent instead →"*. Search results still render below. **Never auto-route, always show evidence and offer escalation.**

**Result types — one feed, six card shapes.** Clip (waveform sliver, in-card play, timestamp), Episode (cover art, show name, duration, *Why it matched* snippet), Show (logo, episode count, follow state), Topic (typographic card to UX-04), Person (portrait or initial monogram to UX-13), Transcript moment (pull-quote treatment, speaker attribution).

**Scope chips** (horizontal, scrollable, multi-select with one exclusive group): *This show · This week · This month · Unlistened · By [Person] · On [Topic] · In transcripts · In wikis*. When the user types a name we recognize ("Tim Ferriss"), a `By Tim Ferriss` chip morphs into the rail via `glassEffectID`.

**Typeahead with semantic suggestions.** As the user types `keto`, two sections: *Literal matches* (3 episodes whose titles contain "keto") and *Semantic neighbors* (clips about ketosis, fasting, low-carb), the latter with a small `~` glyph.

**Long-press peek.** Any clip / transcript card → glass sheet rises 2/3 height showing 60 seconds of transcript surrounding the match, with the match line glowing. Release dismisses; drag up commits to the full transcript view.

**Voice search.** Holding the mic dims the rest of the screen to 30 %; the search field becomes a glass capsule that pulses with the audio waveform. Releasing without speaking dismisses; speech commits the query; results stream in beneath while still listening.

**No-results escalation.** *"Not in your library. Search the open web?"* card → fires `perplexity_search`. A tertiary action *Find a podcast that covers this* → fires `find_similar_episodes` against the open-web result's topic.

**Result rendering.** Each result type gets a slightly different glass treatment so the eye learns to scan: clips on capsule glass with waveform watermark, episodes on 16 pt-corner glass with cover-art bleed, shows tinted with the show's dominant color (8 % opacity), topics paper-feel hairline, transcript moments italic with oversized open-quote glyph. **One container, morphing.** All wrapped in `GlassEffectContainer(spacing: 12)` so cards near each other blend on scroll bounce — the library feels physically continuous.

**Recents privacy.** Recents persist in App Group store. Per-tab clear; *Incognito search* toggle (no logging, no embedding cache) surfaced in chip rail when active so users don't forget.

**Edge cases.** Transcription backlog: card with hairline progress shimmer (*"Transcript indexing — 73 %. We'll re-rank when ready."*); never hide the episode, never lie. Stale embeddings after model upgrade: background re-index; search still works on old index. Offline: search runs locally against the cached vector index with banner.

### 5.8 AI Briefings / TLDR Player

> Source: [docs/spec/briefs/ux-08-briefings-tldr.md](briefs/ux-08-briefings-tldr.md)

A briefing should not feel like a podcast you happened to receive. It should feel like a podcast that was **made for you, this morning, by someone who has been listening on your behalf.** Visceral target: *NPR's All Things Considered crossed with The Atlantic's print layout.* This surface is the agent's **stage** — the only place it is the producer, not the assistant.

**Object model.**
```
Briefing
 ├─ intro sting
 ├─ Segment[] (title · TTS body · original-audio quotes? · sources[])
 │    └─ Branch[] (forks; Briefing-shaped sub-objects with breadcrumb back)
 ├─ outro sting
 └─ metadata: scope, length_target, generated_at, sources[]
```

**Branch contract: pause-and-resume**, not fork-and-replace. The main thread freezes at the sample the user spoke over; the branch plays as a parenthetical; on completion or *back*, main resumes from that sample. Branches persist and resurface on re-listen as optional side-paths in the rail.

**Compose surface.** Preset row (Daily / Weekly / Catch-up on… / Topic deep-dive), freeform *"Brief me on…"* field, length puck (3 / 8 / 15 / 25 min), scope chips (*My subs · This show · This topic · This week*), pinned recents.

**Player surface.** **Transcript, chapter list, and segment rail are the same surface in three densities**, not three UIs. Collapsed: horizontal glass strip. Up: chapter list with attribution chips. Up again: full live-transcript auto-scrolling with playback. Active pill morphs into the next via `glassEffectID` while a thin gradient ribbon sweeps under the title (250 ms).

**Material.** Now-Playing uses neutral glass over episode artwork; briefings use a **warm-tinted variant** — `glassEffect(.regular.tint(brassAmber.opacity(0.18)).interactive(), in: .rect(cornerRadius: 28))` — over a slow-drifting generative gradient (warm ink, brass, parchment). **Brass-amber glass = the agent owns this audio.** Briefings titles in *New York Large* 34 pt with a dropcap on each segment's leading word.

**Cinematic intro / outro.** Two-second open: hairline rule draws edge-to-edge, title fades in as the sting plays, rail crystallizes from the rule (the rule *is* the rail's spine). Outro reverses to a point, fades.

**Voice barge-in.** Audio ducks 12 dB. Glass deepens tint with an inner glow on its leading edge — *listening*. Segment title freezes mid-word, italicized. Agent's answer lifts as a second glass card above the rail; on resume it morphs back as a branch crumb. *Return to briefing* chip with a 4 s auto-resume ring.

**Generation in progress.** Stream-as-ready: segment 1 plays before the last synthesizes. Mid-generation cancel offers *"Save partial briefing?"* — partials are valid artifacts with a torn-edge cover motif.

**Dedicated library shelf.** Briefings live in a separate, brass-amber-tinted shelf — never mistaken for an episode. Filter by date, scope, length. 30-day auto-archive unless saved.

**Boundary contract: three briefs, one object.** UX-08 owns the **player** (this section). UX-05 hosts the **briefing card** in chat. UX-14 hosts the **daily edition delivery** (Today + push). The `BriefingComposer` and `BriefingPlayer` engines are owned here; the cards and pushes elsewhere are views onto the same object.

**Edge cases.** Original-audio fetch fails: substitute paraphrased TTS, mark the chip *paraphrased* — never silently drop a citation. Briefing too long for corpus: agent counter-proposes (*"12 min on this, or 25 with adjacent shows"*). Unfeasible scope: empty state offers a `perplexity_search` *web briefing* with a cool tint, distinct texture, labeled out-of-corpus. Already-heard segments flagged with badge; setting controls skip vs include.

### 5.9 Cross-Episode Knowledge Threading

> Source: [docs/spec/briefs/ux-09-cross-episode-threading.md](briefs/ux-09-cross-episode-threading.md)

The connective tissue. A second voice in the margin: calm by default, alive on demand. Where the wiki destination is the **library**, threading is the **librarian who finds you in the stacks**.

**Three layers, never a tab.**
- **Layer A — Now Playing Context Ribbon.** Thin, dismissible glass strip pinned to the bottom of the player above the transport controls. Appears only when the agent has detected an active topic with **≥3 prior mentions**. Auto-fades after 6 seconds if ignored. Single counter glyph: *"7 ↺"*. **Cap at 1 ribbon per 10 minutes of listening, never within 30 s of a chapter boundary or user action.**
- **Layer B — Transcript inline citations.** Topics get a thin parchment underline (1 px hairline) in editorial serif; long-press reveals a peek at 40 % detent. Coordinated with UX-03's text styling; we provide the underline token + long-press behavior.
- **Layer C — Thread Detail Sheet.** Full-screen modal sheet (not a destination page); dismiss with swipe-down. Three tabs: **Timeline · Contradictions · Evolution**. Defaults to whichever has the most signal.

**Color semantics.** Threading neutral = parchment underline (#E6DCC8 light / #3A352B dark). Contradiction = amber seam (#D9A441), 2 px, animated shimmer-once on first appearance. Evolution = gradient cool (#6B9BD1) → warm (#D88A5C) along chronological axis. Confidence-dim = 50 % opacity with dotted, not solid, underline.

**Confidence vocabulary.** Below 0.75 confidence, contradictions render with a dotted amber underline and a footer reading *"Agent's read — may not be a true clash."* Above 0.9, solid seam, no caveat. We never assert contradiction with certainty unless verbatim quotes oppose on the same noun phrase.

**Sparse-evidence rule.** No ribbon, no underline below 3 mentions. Threading is reserved for *patterns*, not coincidences.

**Scrub-the-timeline microinteraction.** In the timeline tab, horizontal strip of clip pills. User drags a finger across; a magnifier glass capsule (true `glassEffectID` morph) rides under the finger, expanding the hovered pill 1.4× and previewing 2 seconds of audio at low volume. Release commits → opens player at that clip.

**Boundary with UX-04.** Open *wiki entry for "ketogenic diet"* button at the bottom of every detail sheet is the primary reinforcement of the threading-vs-wiki boundary: episodic recall here, synthesized knowledge there.

**Boundary with UX-13.** Speaker chips in transcript long-press into UX-13 peek sheets (UX-13 owns); threading owns the underline + long-press *behavior* on topic chips.

### 5.10 Onboarding & First Run

> Source: [docs/spec/briefs/ux-10-onboarding.md](briefs/ux-10-onboarding.md)

The first five minutes must feel like the app was waiting for *this user specifically*. Onboarding's job is not to explain the app; it is to demonstrate that the app already understands them. By the time the first briefing fades, the user has already used four of the deepest features — import, identity, agent, voice — without ever feeling configured.

**Resolution of the discriminating decision: trial budget (Option A).** First briefing runs on a small house-funded LLM + TTS budget (~$0.05/user, device-attested, capped at one briefing + ~2K agent tokens). BYOK is gently introduced *after* the user is hooked, at the second substantial agent action. Option B (text-first) ships fast but loses the magical moment; Option C (BYOK before the magic) is a death spiral. PROJECT_CONTEXT's "BYOK, no default key" stance is honored everywhere except this single onboarding window.

**State machine.** S1 Welcome → S2 Import (OPML / clipboard / starter pack) → S3 Identity (auto, ~3 s) → S4 Agent (trial-on, BYOK soft) → S5 Voice persona (optional) → S6 First Briefing (mic permission requested in-context here) → S7 All Set.

**S1.** A single editorial sentence over a moving gradient — *"Talk to all your podcasts."* No logo splash, no carousel. Power-user escape: low-contrast *"I know what I'm doing"* link → 30-second condensed flow (paste OPML, paste OpenRouter key, done).

**S2 — the detection moment.** OPML imports and 47 shows fan out as the user watches; ovals are real artwork crops, not placeholders. Total animation: 1.4 s. Empty library → curated 12-show starter pack. *"Auto-detect from Apple Podcasts" is platform-impossible — we are honest in copy.*

**S3 — the quiet promise.** Identity generated invisibly. A constellation animation; one line: *"This stays on your device."* No keys shown unless asked. The term "nsec" never appears outside Settings.

**S4.** The agent is introduced as having *"read every episode you subscribe to"*; trial budget explained in two sentences; *"I have a key already"* opt-out present.

**S5.** Three voice persona cards (Aria / Kai / Sage) with one-tap preview. Skippable; default Aria.

**S6 — the magical moment.** *"Your week, in 4 minutes."* Briefing intro draws a horizontal timeline of dots (one per included show); play button breathes (subtle scale 0.98 ↔ 1.00, 2 s cycle). Mic permission requested in-context; on deny, briefing plays through normally and the *"Tap to interrupt"* pill becomes *"Tap to ask"*. **First audio chunk must stream within 6 seconds or the magic dies.**

**S7.** *"You're set."* Three checks: 47 shows imported, agent active (trial), daily briefing 7:30 AM (changeable). *Open app →*.

**Quiet Mode (BYOK declined / trial expired).** Playback, transcripts, library work fully; agent and briefings degrade to **read-only summaries from cached metadata**. A persistent but unobtrusive banner offers the BYOK walkthrough. **No feature is hidden, only agent intelligence is paused.**

### 5.11 Ambient Surfaces

> Source: [docs/spec/briefs/ux-11-ambient-surfaces.md](briefs/ux-11-ambient-surfaces.md)

Phone, watch, car dashboard, and AirPods are one product — a single agent always one squeeze away. The screen-off experience is not a stripped-down app; it is the app's **resting state**. Three actions are always one gesture away: **play, ask, brief me.**

**Lock Screen Live Activity (playing).** `GlassEffectContainer(spacing: 24)` with art tile + transcript line in 15 pt SF Pro Text on `.regular` glass. Tint sampled from album art (dominant + complement) so the activity feels native to *this* episode. **Throttle transcript-line updates to one per transcript-segment boundary (~6–10 s), not per word** — ActivityKit budgets ~16 high-frequency updates/hr.

**Dynamic Island states** morph via `glassEffectID` + `@Namespace`: idle → playing (compact: art L, waveform R) → expanded (title + transcript line + scrub + ask) → thinking (three orbs unioned via `glassEffectUnion`, pulsing L→R) → answer-ready → playing. Nothing fades; everything morphs.

**Home widgets** respect `widgetRenderingMode`. In `.accented`, artwork uses `widgetAccentedRenderingMode(.monochrome)`; briefing waveform stays accent-tinted. Small: now playing or *Brief me*. Medium: + 2 briefings + top thread. Large: + 3 threads + ask box.

**Lock Screen widgets.** Rectangular: *Ask agent* deep-link. Circular: play/pause or briefing-ready badge.

**CarPlay.** Strictest contrast environment. **No Liquid Glass blur — the API doesn't expose it and driver attention demands flat.** Min 22 pt body, 34 pt title Semibold, dark `#0A0A0F` backgrounds. Persistent voice mic button bottom-right of every screen. Pulses *only while agent generates* — not while listening (driver feedback is auditory). **No ambient barge-in by default in CarPlay** — false-positive risk at highway speed; PTT only via steering wheel + Siri-style alert tone.

**Apple Watch.** Tonal SF Symbols, full-bleed art on the player, corner complication uses `.accentedRenderingMode(.full)`. Crown scrubs at two rates by velocity: shallow ±5 s, aggressive ±30 s (chapter-jump). Watch standalone RAG is **not in v1** — voice mode degrades to "ask anyway, syncs when phone reachable" with clear UX.

**AirPods.** Single squeeze = play/pause (system default; do not override). Double = skip, triple = previous. **Long press = voice mode** (user opts in during onboarding; one app system-wide can claim it). Soft chime in the buds when listening — distinct from Siri's so blind users can disambiguate.

**Action Button + Lock Screen control + Siri Shortcut + Spotlight + CarPlay button** all share a single `StartVoiceModeIntent`. Document the AirPods setup once in onboarding; users assign in Settings → AirPods → Press and Hold AirPods → Shortcut.

**Boundary.** UX-11 owns the *persistent surfaces*; UX-14 owns *notifications* (which are events, not surfaces). UX-06 voice and UX-08 briefings *render into* the ambient surfaces; we own the data contract.

**Privacy on Lock Screen.** Settings → *Hide transcript on Lock Screen* (default ON for episodes the user flags sensitive). Briefing-rendering Live Activity copy uses *"Preparing your briefing…"*, never *"agent generating."*

### 5.12 Nostr Communication

> Source: [docs/spec/briefs/ux-12-nostr-communication.md](briefs/ux-12-nostr-communication.md)

Nostr is **not a tab. It is a relay layer** that lets people — and the agents they trust — exchange podcast knowledge as if they shared one library. A clip you send a friend lands in their app as a *playable, transcript-aware artifact*, not a URL. A question your friend's agent asks of your library returns prose, not a payload. A command you fire from your laptop arrives at your phone as a normal agent reply with a small *via desktop* glyph.

**Three principles.** Provenance is ambient, never a banner. Trust is tiered, not binary. The wire is invisible until it matters.

**Permission tiers as glass weight.** *Reader* (queries only) is a thin, low-saturation capsule. *Suggester* (drafts that need user approval) has medium tint with soft glow. *Actor* (full tool access, scoped by per-tool overrides) is `.glassProminent` with a tint matching the friend's accent. **Visual weight matches power.** Switching tiers is a morph (`glassEffectID`), never a segmented control.

**Two voices in one thread, four message kinds.** My human messages: right-aligned tinted glass capsule (system accent at 18 %). Friend's human: left-aligned neutral glass with friend avatar at 24 pt. **My agent: left-aligned, unbubbled editorial serif (matches UX-05).** Friend's agent: same, with a hairline vertical glass rule on the leading edge tinted to friend's accent + small-caps eyebrow `Maya's agent · 14:02`. **The eyebrow is the trust signal:** human-from-friend has none (we trust faces); agent-from-friend always carries one (we verify machines).

**Provenance chips.** 9 pt mono caption with a 6 pt circular avatar, prefixed `via`. Always at the trailing edge of the metadata row, never in headlines. Tap → sheet with original Nostr event id (copyable), the tool call, and *Revoke this action* button if reversible.

**Permissions infrastructure already present in the template.** `Friend.identifier` is hex pubkey; `NostrPendingApproval` queues first-contact handshakes; `NostrRelayService` (WebSocket + reconnect + ACL) and `AgentRelayBridge` already run the agent loop for inbound DMs. We extend with `permissionTier: PermissionTier` and `toolOverrides: [String: Bool]` on `Friend`.

**Cross-device own-DMs.** From a Nostr client on desktop you send your own npub a DM ("Make a 12-min briefing for tomorrow's commute"). Phone agent runs `generate_briefing`, replies on the same thread. Phone chat shows the message with a `􀙗 desktop` glyph — same prose, different origin. *My Other Devices* is its own pinned thread.

**Tool-exposure audit (security-critical).** Not every tool is safe to expose to friend-pubkey-driven calls. Default exposures:
- **Reader tier:** `query_transcripts`, `query_wiki`, `summarize_episode`, `find_similar_episodes`. No mutations.
- **Suggester tier:** above + drafts of `play_episode_at`, `generate_briefing` that surface as approval cards on the user's device.
- **Actor tier:** all of the above + `play_episode_at`, `set_now_playing`, `generate_briefing`, `send_clip` — gated by per-tool overrides.

**npub QR reveal.** Cinematic moment: card lifts off its row, expands to fill the screen, QR draws on with a 320 ms staggered shimmer (rows of QR modules cascade in). Tap dismisses with reverse morph. A single line below: `npub1…7q9` in mono with copy glyph.

### 5.13 Speaker & Topic Profiles

> Source: [docs/spec/briefs/ux-13-speaker-topic-profiles.md](briefs/ux-13-speaker-topic-profiles.md)

A podcast library is not a collection of episodes — it is a collection of **voices and ideas**. The same names recur across years; the same arguments mutate from show to show. Today's apps are blind to this; we refuse that. **A speaker is a first-class noun in this app. So is a topic.**

**Profiles are destinations, not summaries.** UX-04 wiki holds the encyclopedic three-line bio + headline links and *opens into* these profile pages. The wiki is the index; we are the article. Profiles are pushed onto the navigation stack of whichever tab summoned them — never their own tab.

**Speaker profile IA.** Header (3:4 portrait hero, name, role pill, stats row, *Follow ●* + *Brief me* + *Share*) → Bio (LLM-generated, sourced) → Most-discussed topics (chip cloud, weighted) → **Stance evolution** (per topic, expandable; each card plays a 12 s pre-roll + 30 s body + 5 s post-roll on tap) → Recent appearances (rail, reverse-chronological) → Best clips (agent-curated, 3–5) → Sources & corrections.

**Topic profile IA.** Header → Definition → Speakers who discuss it (faces row) → Episodes that discuss it (rail) → Timeline (when first surfaced + key inflection points) → **Contradictions** (paired-quote cards) → Subtopics & parent topics (graph chips) → Sources & corrections.

**Follow / notification settings** live as a peek-up sheet from the *Follow ●* pill, **not a separate screen**. New-appearance toggle, delivery (Proactive feed / Push / Both), confidence threshold slider, mute (1 day / 1 week / forever).

**Photo treatment.** Hero portrait at 3:4, 280 pt tall on iPhone, 24 pt corner radius, soft gradient bleed of the photo's extracted dominant color into the page background (luminance clamp for AA contrast — same algorithm as UX-02 show-detail header). When no photo: **initials monogram** (two letters, `.largeTitle` rounded bold) in a circular glass disc tinted from the speaker's most-frequent show's accent. **Never use a generic silhouette stock graphic.**

**Speaker identity resolution (the load-bearing problem).** Diarization gives voices; matching to a *named identity* across shows is hard. Tiered resolver: RSS show-notes first → transcript NER + co-reference → voiceprint clustering across the library → user's disambiguation choices fed back. Confidence surfaced via the threshold slider in Follow settings. Unknown guests offered *Help name this voice*: 4 s clip play, three candidate names suggested, user picks or types.

**Ambiguous identity (two people, same name).** Disambiguation chooser at page open: 2–3 candidate cards (portrait, role, last show context). User's pick remembered per-show context as a tiebreaker; resolver updated. If the chooser cannot be presented (incoming deep link), default to highest-confidence candidate with non-modal banner: *"Did we pick the right Sarah Chen? [Switch]"*.

**Photo licensing.** Prefer Perplexity-cited Wikimedia / Commons sources with explicit license metadata; fall back to verified social avatar (per-platform terms apply); fall back to monogram. Never mass-cache without per-image attribution. *Suggest correction* path lets a guest replace their own photo. Legal review required before ship.

### 5.14 Proactive Agent & Notifications

> Source: [docs/spec/briefs/ux-14-proactive-agent-notifications.md](briefs/ux-14-proactive-agent-notifications.md)

Most apps treat notifications as a megaphone. We treat them as a **front page** — one daily edition, edited by the agent, delivered with the calm of a paper landing on the porch.

**Three principles.** **One push a day, by default.** Everything else pools in Today, the in-app digest. Spam is a design failure, not a settings problem. **Today is editorial, not a feed** — magazine cover, one hero, three to five cards, generous whitespace, *finishable* in under a minute. **Confidence is visible** — solid rule for grounded, dashed rule and italic eyebrow for inferential.

**Insight Card taxonomy.**
| Type | Eyebrow | Confidence | Default Push |
|---|---|---|---|
| Briefing | `MORNING EDITION` | solid rule | yes (1/day) |
| Drop | `NEW · <show>` | solid rule | no |
| Thread | `THIS WEEK · CROSS-EPISODE` | solid rule | no |
| Echo | `YOU ASKED · <date>` | dashed, italic | no |
| Friend | `FROM <name> ·` Nostr glyph | solid rule | priority-only |
| Transcript | `READY · TRANSCRIPT` | solid rule | never |

**Smart Push Budget.** Default 1/day; max 3; off. Surplus pools in Today / Inbox. After day 7, surface a tooltip: *"Want more? Raise your push budget in Settings."* Never auto-raise.

**Confidence thresholds.** Echoes surface ≥0.65; solid rule ≥0.80. *This wasn't useful* taps are ground-truth ranking input; three taps in 30 days mutes that type for a week with confirmation.

**Vacation Mode.** Today and pushes pause; Inbox accumulates. On resume, hero is an *Editor's Note*: *"You were away 6 days. 23 items waiting; here are the 3 the agent thinks matter."* Gap-day editions remain via pull-down (7-day window).

**Background scheduling reliability.** `BGAppRefreshTask` is opportunistic. Trigger briefing generation at 4 AM via APNs background push; on-device fallback composes lazily on first open if missed. Briefing failures **never push broken content** — user opens to *"Briefing didn't compose this morning — try generating one now?"* with one-tap retry.

**Cross-device dedup.** APNs collapse-id keyed to `{user, edition_id}` so iPad + iPhone never double-push.

**Live Activity policy.** Briefings only. Briefings >12 minutes risk system kill — hand off to Now Playing chrome at 80 % completion. Non-briefing items never use Live Activities.

**Privacy.** Push body never reproduces the original question text (Echo cards) — user must open Today.

**Boundary contract.** UX-14 owns the *Today* editorial surface, the *Inbox*, push, and notification settings. UX-08 owns the briefing player itself. UX-13 emits `speaker_appeared` and `topic_referenced` events that this surface consumes. UX-12 emits friend-clip events keyed to a Priority bit per friend (≤1 push/friend/24h).

---
