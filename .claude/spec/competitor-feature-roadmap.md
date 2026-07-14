# Competitor-Inspired Feature Roadmap

## Method

Synthesized 10 competitor briefs (Spotify, Apple, Overcast, Pocket Casts, Castro, Snipd, Fountain, YouTube, Airchat, Podimo). For each candidate I asked: does it amplify our pillars (agent / RAG / voice / Nostr / Liquid Glass) or fight them; does its absence break a marquee user story; how big is the engineering on top of what's shipped (mini-player, voice mode, transcript ingest + RAG, briefings, agent chat, Nostr infra, wiki, search-with-transcript-hits); and does it widen the moat or just buy parity. Features were deduplicated across competitors — Smart Speed / Trim Silence / Enhance Voices collapse into one item; tap-to-seek transcript appears in five briefs and becomes one. Where every competitor agreed it was usually parity; where exactly one had it (Castro's Inbox, Fountain's Nostr, Airchat's auto-play river) I weighed it for differentiation upside.

No invented features — everything traces to a brief. Items already shipped in the codebase are noted inline.

## Scoring rubric

- **Pillar fit (1–5)** — does it amplify AI / voice / Nostr / Liquid Glass identity, or fight it?
- **User impact (1–5)** — how much does it move a marquee story (play-the-keto-part, TLDR-this-week, friend-DMs-the-agent, lean-back-briefing) forward?
- **Effort (S/M/L)** — engineering size on top of shipped surfaces.
- **Differentiation (1–5)** — does it widen moat vs Snipd/Spotify, or just achieve parity?
- **Composite priority** — P0 (must, this round), P1 (should), P2 (could), P3 (won't this round).

Rule of thumb: parity-only items max out at P1. P0 requires either pillar fit ≥ 4 OR a marquee-story blocker. Differentiation alone with weak pillar fit caps at P2.

## The Roadmap

### Bucket A — Audio engineering table stakes

The non-negotiable foundations. Power users will dismiss us in seconds without these.

- **Smart Speed + Voice Boost (audio engine pass)**
  Sources: Overcast (canonical), Castro, Pocket Casts. Real-time silence compression + ITU BS.1770 loudness normalization to ~-14 LUFS with a true-peak limiter. "Time saved" counter in the player. Marco's 2020 post is the spec.
  Why now: Spotify's #1 churn-to-Overcast trigger; we don't have it (audited `AudioEngine.swift`).
  Effort: M. Risks: DSP correctness; gate Voice Boost behind a toggle.
  Score: pillar 3 / impact 5 / diff 2 / **P0**

- **Per-show playback overrides (speed, silence, voice boost, episode limit/order)**
  Sources: Pocket Casts, Castro, Apple. Per-podcast settings overriding globals; agent reads them as structured context so "always play Lex at 1.4x" works in voice.
  Why now: cheap once the audio engine lands; marquee voice commands depend on the data model.
  Effort: S after Smart Speed.
  Score: pillar 4 / impact 4 / diff 2 / **P0**

- **CarPlay + Apple Watch + AirPlay 2 parity**
  Sources: Apple, Castro, Pocket Casts, Overcast. CarPlay scene (lock-screen now-playing already wired in `NowPlayingCenter.swift`) + Watch with queue + remote control.
  Why now: car/commute is the dominant listening context; voice-mode-in-CarPlay is uniquely ours.
  Effort: L. Risks: review queue.
  Score: pillar 4 / impact 5 / diff 3 / **P1**

- **Chapter-aware sleep timer + voice-extend**
  Sources: Overcast, Apple, Castro, Pocket Casts. Sleep timer sheet exists (`PlayerSleepTimerSheet.swift`); add chapter-boundary mode and voice-extend ("hey, keep playing") as the upgrade of Pocket Casts' shake.
  Effort: S.
  Score: pillar 4 / impact 3 / diff 3 / **P1**

- **Configurable headphone-control mapping**
  Source: Snipd (its #1 deal-breaker is hijacking triple-click). User picks double/triple/long-press → snip vs. skip-back vs. next-chapter from day one.
  Why now: unblocks ambient snipping (Bucket B) without inheriting Snipd's worst review.
  Effort: S.
  Score: pillar 3 / impact 4 / diff 4 / **P0**

### Bucket B — AI-native features that ARE the product

This is where we win. Every item here only exists because of the agent + RAG + voice infra.

- **Cross-episode RAG chat (library-wide ask-anything)**
  Sources: Snipd (per-episode chat is its biggest gap), Apple (per-episode transcript search is its biggest gap), Pocket Casts. `RAGService.swift` exists. Surface as the default chat mode: "what has Huberman said about magnesium across all episodes" → cited segments with one-tap play_episode_at.
  Why now: the Snipd-killer. Their chat has no memory beyond one episode; ours spans the library by construction. Without this we're just another player with embeddings.
  Effort: M (RAG exists; needs cross-episode result UI + citation peek).
  Score: pillar 5 / impact 5 / diff 5 / **P0**

- **AI Inbox triage (Castro × agent)**
  Sources: Castro (Inbox concept), Apple (broken Up Next confirms demand). Two-stack model — but the agent triages autonomously, surfacing only episodes worth a human decision and routing the rest via per-show AI profiles. "Review mode" shows what the agent did and why ("auto-archived: you skipped the last 4 of these"). Every routing decision carries a one-line rationale (Spotify Prompted-Playlist pattern).
  Why now: subscriptions snowball; without triage the queue rots. The daily editorial surface where the agent's intelligence becomes visible.
  Effort: M (UI is Castro-derived; ranking is the work).
  Score: pillar 5 / impact 5 / diff 5 / **P0**

- **TLDR briefing as lean-back river (Airchat × Briefings)**
  Sources: Airchat (auto-play feed), Podimo (For-You daily brief), Snipd (AI DJ). Briefings already exist (`BriefingPlayerView.swift`). Add Airchat's auto-advance: open → audio starts, haptic punctuation between cards, transcript scrolls under the waveform, barge-in to branch ("more on the keto one"). Phone face-down, the experience still works.
  Why now: TLDR-this-week is currently tap-tap. Airchat's architecture makes it a daily ritual.
  Effort: M.
  Score: pillar 5 / impact 5 / diff 5 / **P0**

- **Tap-to-seek transcript + long-press → ask-agent-about-this**
  Sources: Apple (canonical), YouTube, Pocket Casts, Spotify. `PlayerTranscriptScrollView.swift` exists; missing is word-level timing and a long-press affordance that opens chat pre-loaded with that timestamp as context. Every paragraph becomes an agent prompt.
  Why now: the agent's intelligence has to be touchable.
  Effort: M (word-level Whisper, long-press handler, context injection).
  Score: pillar 5 / impact 4 / diff 4 / **P0**

- **Mentioned entities — books, guests, other podcasts**
  Sources: Apple (mentioned podcasts iOS 26.2), Snipd (mentioned books, guest pages). Transcript NER pass builds entity cards. In-transcript and in-chat: "Andrew mentioned _Why We Sleep_ — also mentioned 7× across your library" with one-tap follow / open-wiki / find-similar.
  Why now: durable knowledge-graph nodes feed the LLM-wiki pillar.
  Effort: M.
  Score: pillar 5 / impact 4 / diff 4 / **P1**

- **Personal "Most Replayed" graph**
  Sources: YouTube (mass-scale), Snipd (Hive Brain). Plot the user's own replay/pause/seek density on the scrubber. Agent uses peaks as another "what mattered" input. Federate later (opt-in, Nostr) into community heatmaps.
  Why now: turns the player's own behavior into intelligence; no competitor does the personal version.
  Effort: M.
  Score: pillar 4 / impact 3 / diff 5 / **P1**

- **Voice onboarding (agent interview replaces the quiz)**
  Source: Podimo (8-step quiz proves the pattern). After OPML import the agent asks 3–4 questions: which shows you finish, which hosts you skip, what topics you wish you heard more. Populates per-show profiles and seed embeddings.
  Why now: shows the agent and voice mode in the first 60 seconds — the aha-before-paywall move.
  Effort: M (rides on voice mode polish that just landed).
  Score: pillar 5 / impact 4 / diff 5 / **P1**

### Bucket C — Social / Nostr-native moats

Where Nostr + Lightning + agent give us a structural edge no closed app can copy.

- **Nostr-published timestamped comments**
  Sources: Fountain (canonical), YouTube (timestamp-comment behaviour). Comments pinned to a transcript span, published as Nostr events. Survives the app, interops with Fountain from day one. Nostr infra exists (`NostrRelayService.swift`, `Nip46/`).
  Why now: only Fountain ships this, and they're stuck on the Bitcoin-only onramp. Text-only is weeks of work and immediately differentiates.
  Effort: M.
  Score: pillar 5 / impact 4 / diff 5 / **P0**

- **Friend-DMs-the-agent (Nostr DM → tool-call)**
  Source: Podcastr pillar; Fountain proves audio-Nostr audience exists. A friend's DM ("send me the 3-min keto bit from yesterday's Tim") enters the agent's tool loop and fires `find_similar_episodes` + clip-share. Same bus lets a user's other devices command the agent.
  Why now: the only social mechanic in the competitive set nobody can copy without our exact stack.
  Effort: M (NIP-44 ingress → agent transport adapter).
  Score: pillar 5 / impact 5 / diff 5 / **P0**

- **3-fidelity clip share (quote card / waveform video / audio)**
  Sources: Snipd (3-tier share is its viral loop), Overcast (overcast.fm web pages), Fountain (transcript clip editor), Apple/Spotify (timestamp deep-link). Agent picks boundaries from a transcript span; renders a Liquid Glass quote card, animated waveform video, or audio clip. Universal-link recipient gets "Play from XX:XX"; clip lives at a public URL whether or not they have Podcastr.
  Why now: top-of-funnel growth. Every "play the keto part" answer ends with a one-tap share that looks gorgeous.
  Effort: M.
  Score: pillar 5 / impact 5 / diff 4 / **P0**

- **Spaced-repetition daily recap widget**
  Source: Snipd. Saved snips and agent-flagged moments resurface in a home-screen widget on a Leitner schedule. Knowledge-graph as retention loop.
  Effort: S.
  Score: pillar 4 / impact 3 / diff 4 / **P2**

### Bucket D — Editorial / Liquid Glass UX wins

Where craft is the feature. Competitors lose on visual identity.

- **Now-Playing card with editorial art backdrop + Liquid Glass material**
  Sources: Castro (universally praised), Apple. Artwork-derived gradient + iOS 26 Liquid Glass on controls, combined with the existing chapter scroller and waveform views for one cinematic surface.
  Why now: Castro's visual reputation came from this single decision; Liquid Glass lets us leapfrog it.
  Effort: S–M.
  Score: pillar 5 / impact 4 / diff 4 / **P0**

- **Chapter ticks in scrub bar + agent-named auto-chapters**
  Sources: Apple (iOS 26.2 auto-chapters), Spotify (PODTILE), Castro, Overcast, YouTube. `PlayerChaptersScrollView.swift` exists; missing is chapter break ticks on long-press scrubbing and AI-generated chapters where RSS doesn't supply them — agent-named so they read editorial, not "Chapter 3."
  Why now: foundation for "play the keto chapter" voice command and pre-scan UX (Snipd's biggest win).
  Effort: S (UI) + M (generator).
  Score: pillar 4 / impact 4 / diff 3 / **P1**

- **Swipe-to-triage gesture system in Inbox**
  Source: Castro. Swipe-right → queue, swipe-left → archive, long-press → "TLDR it." Maps to our three consumption modes (listen / brief / skip).
  Effort: S.
  Score: pillar 4 / impact 4 / diff 3 / **P1**

- **Per-result rationale chips ("why this is here")**
  Source: Spotify (Prompted Playlists). Every agent-curated artifact — briefing entry, queue addition, search hit — carries a collapsible one-line "why" chip. The agent already generates this text.
  Why now: counters "AI as black-box"; near-free given existing infra.
  Effort: S.
  Score: pillar 5 / impact 3 / diff 4 / **P1**

### Bucket E — Anti-patterns to ban

- **Unified music+podcast queue (Spotify).** Structural bug factory. Podcast queue stays a first-class object owned by the agent.
- **Hijacking AirPods controls without opt-out (Snipd).** Triple-tap is skip-back muscle memory. Mappings ship configurable from day one (Bucket A item 5).
- **Per-episode-only chat (Snipd).** Our wedge is cross-episode. Library-wide is the default; per-episode is the special case.
- **Processing caps as the paywall (Snipd's 900 min/mo).** Heavy listeners hit it on day three and feel punished. Gate features, not minutes.
- **Auto-snips / auto-folders without consent (Snipd, Pocket Casts Smart Folders Apr 2025).** Every AI artifact is opt-in or surfaces in review state.
- **Discovery-first home (Apple Listen Now, Spotify Home).** Default view is the user's world; recommendations carry explicit provenance.
- **Smart-Playlist rule UIs (Pocket Casts, Overcast).** The agent IS the playlist; natural language beats a form-builder. We won't ship the form.
- **Bitcoin-as-prerequisite (Fountain).** Nostr text features ship without a wallet. Lightning is opt-in if at all.
- **Wrapped-only stats (Spotify, Pocket Casts).** Annual recap is a marketing stunt; ours are always-on and agent-queryable.
- **Shipping a rewrite without parity (Overcast 2024).** When we touch the playback engine for Smart Speed, ship behind a flag and keep the old path until parity is proven.

## Top 10 — what to build first

1. **Cross-episode RAG chat surface.** The Snipd-killer; our entire wedge is library-wide chat the agent can act on.
2. **Smart Speed + Voice Boost.** Without these, every Overcast power user dismisses us in 30 seconds — and they own the influencer layer.
3. **AI Inbox triage (agent-driven Castro two-stack).** Solves the queue-rot problem every other app has and makes the agent's intelligence a daily editorial surface.
4. **Friend-DMs-the-agent over Nostr.** The only social mechanic no competitor can copy without our exact stack — uniquely ours, ship it loud.
5. **Lean-back briefing river (auto-play TLDRs, transcript-under-waveform, barge-in).** Turns TLDR-this-week from a tap-tap into a daily ritual; Airchat architecture proven at scale.
6. **Tap-to-seek transcript with long-press → ask-the-agent.** Apple proved demand; we make every paragraph a prompt for the agent, not just a seek target.
7. **Liquid Glass Now-Playing card with editorial art backdrop.** Castro-class visual identity, leapfrogged by iOS 26 material — the screenshot that recruits users.
8. **3-fidelity clip share (quote card / waveform video / audio) with universal-link "Play from XX:XX."** Top-of-funnel growth loop and the natural payoff to every "find me the X part" agent answer.
9. **Nostr-published timestamped comments.** Lightning-free, ships in weeks, instantly interoperable with Fountain's audience and structurally uncopyable by closed apps.
10. **Configurable headphone-control mapping + per-show playback overrides.** Two small items that together unblock ambient snipping (Bucket B) and surface the agent's per-show context model.

## Open questions for the user

1. **Inbox model: explicit user triage or autonomous agent triage?**
   Castro-style Inbox where the user is the decision-maker (agent proposes), versus agent-decides-by-default with a review mode (user audits). These are very different UIs and onboarding flows; we need to commit before designing the home tab.

2. **Auto-process every subscribed episode for transcripts + embeddings, or on-demand only?**
   Always-on means cross-episode chat answers in <1 s but burns Whisper / embeddings cost on episodes the user may never play. On-demand keeps cost predictable but introduces a "thinking..." latency the first time the user asks anything across the library.

3. **Nostr social this round: text-only comments first, or comments + Lightning boosts together?**
   Text-only ships in weeks and avoids the wallet onramp Fountain stumbles on. Boosts together gives one cohesive launch story but means custodial-wallet UX, regulatory thinking, and a real anti-spam strategy on day one.

4. **Wrapped-style annual recap, or always-on agent-queryable stats only?**
   Wrapped drives a viral December moment (Pocket Casts/Spotify proven). Always-on stats fits our agent identity but has no media event. We can do both, but only one belongs in the first ship.

5. **Web layer scope: shareable clip landing pages on day one, or mobile-only with universal-links-back-to-app?**
   Overcast's overcast.fm pages drive organic installs; building it now means a real web stack. Universal-link-only means viral clips bounce off non-Podcastr users.

6. **CarPlay scope: full app surface, or voice-mode-first CarPlay (the agent IS the CarPlay UI)?**
   Full surface is what Apple expects and what Pocket Casts ships. Voice-mode-first is on-pillar and a unique selling story but Apple's CarPlay templates aren't built for it — we'd be fighting the framework.
