# Podcast Player — Project Context

> Shared context for all UX/research/engineering agents working on the spec phase.

## Vision

A next-generation iOS podcast player built around an embedded AI agent that has **perfect knowledge** of every podcast the user is subscribed to — including episodes they have not listened to yet.

The user can:
- **Talk to all their podcasts** as if they were one continuous conversation.
- Ask things like:
  - *"Hey, play the part of this episode where they talk about keto."*
  - *"Last week I listened to a podcast about stamps or something — what was it?"*
  - *"Make me a TLDR of all the podcasts this week."* → agent generates a TTS audio briefing the user can listen to and **interrupt at any point** to ask follow-up questions.
- Communicate with the agent via **Nostr DMs** (template already supports a Friend agent system) and via **voice mode** (STT in, agent voice out, barge-in friendly).

The bar for UX is *spectacular, absolutely gorgeous*. iOS 26 Liquid Glass aesthetic. Editorial typography. Cinematic motion. We want to set a new bar above Spotify, Overcast, Pocket Casts, Castro.

## Foundations We Inherit From The Template

Located at `/Users/pablofernandez/Work/ios-app-template` (now cloned into `/Users/pablofernandez/Work/podcast-player`).

- **SwiftUI + Tuist**, iOS 17+ baseline, target iOS 26 Liquid Glass design language.
- **AppStateStore** (`@Observable` store, App Group UserDefaults persistence).
- **Agent loop** — tool-calling via OpenRouter. Schema + dispatcher in `App/Sources/Agent/AgentTools.swift`. Extend with new tools.
- **Friend system** — already designed for Nostr-based agent-to-agent communication.
- **Shake-to-feedback**, haptics, glass surface modifier, pressable button style — keep all of this.
- **TestFlight CI** — push to main → GitHub Actions deploys.

## Key Capabilities We Are Adding

1. **Podcast playback** — RSS / OPML import, AVPlayer-backed audio engine, Now Playing integration, lock-screen controls, AirPlay, CarPlay.
2. **Timestamped transcripts with speaker diarization** — use the publisher's transcript when available; otherwise transcribe via ElevenLabs Scribe (or equivalent).
3. **LLM-generated wikis** — in the style of [nvk/llm-wiki](https://github.com/nvk/llm-wiki). Per-podcast and cross-episode knowledge surfaces.
4. **Embeddings + RAG** — vectorize transcripts and wiki pages via OpenRouter; store local vector index. Agent queries this for grounded answers.
5. **Voice conversational mode** — push-to-talk and ambient/always-on. Interruption-friendly: user can talk over the agent's TTS output, agent stops, listens, answers, returns.
6. **TLDR / Briefing audio mode** — agent generates a personalized "catch me up on this week's podcasts" audio briefing as a synthesized podcast episode. Interruptible. Branchable.
7. **Agent tools** to extend `AgentTools.swift`:
   - `play_episode_at(episode_id, timestamp)` — open the player at a specific position.
   - `search_episodes(query)` — semantic + keyword.
   - `query_wiki(topic)` — pull from the LLM wiki.
   - `query_transcripts(query, scope?)` — RAG over transcript chunks.
   - `generate_briefing(scope, length)` — produce a TLDR audio briefing.
   - `perplexity_search(query)` — out-of-corpus online lookup.
   - `summarize_episode(episode_id)` — on-demand episode summary.
   - `find_similar_episodes(seed_episode_id)` — discovery.
   - `open_screen(route)` / `set_now_playing(timestamp)` — UI mutation tools.
8. **Nostr-mediated agent commands** — friend's agent (or your own agent on another device) can DM commands and receive responses.

## Sample Marquee User Stories

- *"Play the part of yesterday's Tim Ferriss where he talked about keto."* → agent finds, opens player at exact timestamp.
- *"What was that podcast last week about stamps?"* → fuzzy semantic recall, opens episode + the relevant clip.
- *"Give me a TLDR of this week's podcasts in 12 minutes."* → generates a TTS briefing on the fly, plays it. User says *"Wait, who was that guest?"* mid-briefing — agent answers from RAG, returns to briefing.
- *"Send my partner a clip of the part where she's mentioned."* → finds her name in transcripts, makes a clip, shares.
- *"What does this podcast say about Ozempic across all their episodes?"* → cross-episode synthesis from transcripts + wiki.
- *"What's a contrarian take on what they just said?"* → grabs current playing position, runs `perplexity_search`, surfaces opposing view.

## Design North Star

- **Editorial**: typography-first, generous whitespace, periodical-quality layouts.
- **Cinematic motion**: rich transitions, parallax, considered timing curves.
- **Liquid Glass everywhere**: dynamic blur, refraction, light/dark adaptation per iOS 26 design language.
- **Audio-first accessibility**: app must be excellent under VoiceOver, with one-handed driving use, with the screen off.
- **Calm by default, alive on demand**: the home screen should feel quiet; the agent and player should feel alive.

## Where Each Agent's Output Goes

- UX briefs → `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-NN-slug.md`
- Research notes → `/Users/pablofernandez/Work/podcast-player/.claude/research/research-slug.md`
- Skeleton work happens directly in repo files (in a feature branch managed by the engineer agent).
- Final synthesized product spec → `/Users/pablofernandez/Work/podcast-player/.claude/spec/PRODUCT_SPEC.md` (assembled after briefs are in).
