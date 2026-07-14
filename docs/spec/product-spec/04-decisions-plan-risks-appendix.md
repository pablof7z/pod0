# Product Spec: Decisions, Plan, Risks, and Appendix

> Part of the Podcastr product spec. Start at [PRODUCT_SPEC.md](../PRODUCT_SPEC.md).

## 8. Cross-Cutting Decisions Already Made

These are settled. Surface briefs that contradict are wrong.

1. **Tab structure: 5 tabs at v1** (Today, Library, Wiki, Ask, Settings) once template Home retires. Skeleton currently has 6 (the sixth is template Home, kept until TestFlight).
2. **Onboarding trial budget (Option A).** First briefing on a small house-funded budget; BYOK introduced after the user is hooked.
3. **Persistence migration order.** v1 `AppState` UserDefaults stays; v1.1 SwiftData lands empty; v1.2+ entities migrate feature by feature.
4. **Agent context strategy: inventory + handles + RAG-via-tools.** No more dumping every item, note, memory into the prompt.
5. **Single `AudioSessionCoordinator`** owns every `AVAudioSession` transition. No exceptions.
6. **Live STT default: Apple `SpeechAnalyzer` on iOS 26+** (we deploy iOS 26). `SFSpeechRecognizer` is fallback; cloud STT is for offline transcript ingest only, never the live loop.
7. **Embeddings: OpenRouter `openai/text-embedding-3-large` @ 1024 d** behind `EmbeddingProvider` protocol; vector store sqlite-vec via `SQLiteVec` Swift package; rerank Cohere `rerank-v3.5`.
8. **Transcription: ElevenLabs Scribe v2 batch + async webhook**, publisher transcripts first, on-device `SpeechAnalyzer` as opt-in privacy mode.
9. **Conversational TTS: ElevenLabs Flash v2.5** WebSocket streaming (~75 ms TTFB). Briefing TTS: ElevenLabs Multilingual v2 / v3 pre-rendered.
10. **Briefing surface ownership.** UX-08 owns the player engine; UX-05 hosts the card in chat; UX-14 hosts the daily edition delivery. Three briefs, one `Briefing` object.
11. **Copper accent (`accent.player`) is exclusive to Now-Playing surfaces.** No other primary CTA in the app uses copper.
12. **Editorial typography is the load-bearing decision.** New York serif for hero + agent prose + transcript body + wiki body; SF Pro for chrome; SF Pro Rounded for chips and the agent voice; SF Mono for timestamps.
13. **One push a day default; surplus pools in Today.** Spam is a design failure.
14. **Wikipedia-style provenance.** Every claim in wiki / briefing / threading points to `(episode_id, start_ms, end_ms)`. Provenance-or-it-doesn't-render.
15. **Quiet Mode is the BYOK-declined fallback.** Playback / transcripts / library work fully; agent and briefings degrade to read-only summaries.
16. **iCloud Drive Markdown mirror** for the wiki is what makes it feel like a real second brain.
17. **Skeleton file layout is final.** Net-new code lands inside the existing module/feature stubs; no `AgentExtensions/` folder; new agent tool dispatchers live in `Agent/AgentTools+{Podcast, RAG, Wiki, Briefing, Web}.swift`.

---

## 9. Open Decisions (need a call before build)

Resolve before sprint zero. Each carries a recommended default; the row says what is undecided.

| # | Decision | Default / recommendation | Why it matters |
|---|---|---|---|
| 1 | **Bundle ID is fixed for App Store continuity.** The app ships as `io.f7z.podcast`; the widget ships as `io.f7z.podcast.widget`. | Keep these IDs for v1 to avoid re-provisioning; treat renames as post-launch migration work. | App Store identity, push topic, App Group are downstream. |
| 2 | **OPML at 2 k+ feeds.** UX-02 flagged perf. | Cap initial parse at 2,000; stream the rest. | Power-user import path. |
| 3 | **Discover regeneration cadence.** UX-02 + UX-14 boundary handshake. | Nightly at 3 AM, plus on-subscription change. | Avoids stale rails without burning compute. |
| 4 | **Auto-scroll lock duration after manual scroll on Now Playing.** UX-01 §9. | Indefinite, until *Return to live* pill tapped (Slack model). | Reading mid-listen is a marquee user moment. |
| 5 | **Agent inline-answer → full-chat threshold.** UX-01 §9. | ≤3 turns inline; *Continue in chat* CTA appears on turn 3. | Player↔chat handoff. |
| 6 | **Citation density per paragraph.** UX-03 §9. | Cap at 2 visible; rest in margin marker. | Reader noise / RAG temptation. |
| 7 | **Wiki disambiguation:** "Cold" the noun vs "Cold open" the chapter. UX-03 + UX-04. | Topic NER prefers entity over chapter; chapter only links if no entity match. | Linking policy for the entire reader. |
| 8 | **Live transcript freshness smoothing.** UX-03 §9. | 8-second smoothing buffer; flag if drift exceeds 800 ms persistently. | 30 s ElevenLabs blocks make the cursor sticky. |
| 9 | **Wake-word on/off at v1.** UX-06 §9. | **Off** at v1 (gesture-only). Hot-phrase "Hey, podcast" at v1.1 with parody guard. | Podcast audio itself contains "Hey, …" |
| 10 | **TTS voice identity choice.** UX-06 §9. | Select branded ElevenLabs Pro voice at launch ("Aria"); user can switch in S5 onboarding. | One-time brand decision. |
| 11 | **AirPods long-press claim.** UX-11 §9. | Off by default; opt-in in onboarding S5; copy explains the system override. | One app system-wide can claim. |
| 12 | **Push budget UX after raising.** UX-14 §9. | Day-7 tooltip offering "Want more? Raise in Settings." Never auto-raise. | Avoid spam-creep. |
| 13 | **Transcript ready over Lock Screen.** UX-14 + UX-11. | Today only, never push. | Privacy + push-budget. |
| 14 | **Photo licensing source order.** UX-13 §9. Legal review. | Wikimedia Commons → verified social → monogram. | Copyright. |
| 15 | **Speaker-resolver confidence threshold default.** UX-13 §3. | `medium`. User can lower to surface plausible matches. | Resolver false-positive rate. |
| 16 | **Per-show *Shareable / Private* toggle for Nostr.** UX-12 §9. | Default Shareable; Health / Finance preset auto-Private. | Library privacy. |
| 17 | **Default Nostr relay set.** UX-12 §9. | 3 relays minimum, parallel writes, first-ack reads, none Apple-/Anthropic-owned. | Single-relay rate-limit risk. |
| 18 | **NIP-44 vs NIP-04 for friend DMs.** UX-12 §9. | NIP-44 mandatory for sensitive content (Health-tagged); NIP-04 fallback otherwise. | Privacy. |
| 19 | **Cross-device delegation key (NIP-26-style).** UX-12 §9. | Per-device, scoped, revocable from My Other Devices. | Don't leak the seed. |
| 20 | **Trial budget per-user ceiling.** UX-10 §10. Finance sign-off. | One briefing + ~2 K agent tokens, device-attested. | Abuse / multi-install farming. |
| 21 | **OpenRouter signup deep-link.** UX-10 §10. | In-app web view (keeps thread); deep-link with referral code. | BYOK conversion. |
| 22 | **Embedding provider Keychain store.** Architecture §8. | Verify OpenRouter `openai/text-embedding-3-large` is reliably available **before** designing around it. If not, add `EmbeddingProviderCredentialStore`. | Vendor coverage. |
| 23 | **Quote-reply visual link in chat.** UX-05 §9. | Defer to v2. | Layout complexity. |
| 24 | **Editorial serif licensing.** UX-05 §9. | New York (system-supplied, free). Custom serif → licensing review. | Cost / risk. |

---

## 10. Phased Delivery Plan

### v1 — Launch (the floor + the magic)

**Baseline.** Every "must" row in §4.1 (variable speed, Smart Speed, Voice Boost, configurable skip, sleep timer, chapter support, AirPlay 2, CarPlay, Lock Screen / Now Playing / Control Center, smart Up Next queue, resume / mark-played, bookmarks, RSS + OPML import-export, iTunes + Podcast Index search, manual + background refresh, downloads + auto-download policy, storage + auto-delete, filter / sort, played-state viz, iCloud sync, Dynamic Type, VoiceOver, reduce motion / transparency, captions / transcripts, in-library + directory search, share-with-timestamp, App Intents + Siri, Live Activity, Watch companion, iPad multitasking, PiP + video, theme + accent, default speed / skip, auto-download defaults, restore-from-backup, analytics opt-out, data delete, per-show cache clear).

**Differentiating.** Five marquee user stories from §2. Now Playing transcript surface (UX-01). Library + Discover (UX-02). Episode Detail + Reading + Follow-Along (UX-03). Wiki browser including Topic, Person, Citation Peek, Generate Page (UX-04). Agent Chat with editorial unbubbled prose, embedded media cards, Tool-Call Inspector (UX-05). Voice mode with sub-second barge-in via SpeechAnalyzer + Flash v2.5 (UX-06). Semantic Search + Topic chips + Voice search overlay (UX-07). Briefings: compose + player + branch contract + library shelf (UX-08). Threading: ribbon + transcript inline + detail sheet (UX-09). Onboarding S1–S7 with trial budget (UX-10). Lock Screen Live Activity, Dynamic Island, small + medium widgets, CarPlay (Audio entitlement, deferred from v1.0 to v1.0.x if entitlement provisioning is slow), Watch standalone playback (UX-11). Nostr friend / friend-agent comms with permission tiers, share-clip, cross-device own-DMs (UX-12). Speaker + Topic profiles with peek sheet, Follow, Brief-me handoff (UX-13). Today + Inbox + 1-push-default + Insight Card taxonomy (UX-14). Liquid Glass design system: AppTheme tokens, Sounds.swift, Haptics extensions, AgentOrb component (UX-15).

### v1.1 — Fast-follow (~6 weeks post-launch)

**Baseline.** Volume normalization, long-press scrub, shake-to-extend sleep timer, Handoff iPhone↔iPad↔Mac↔Watch, episode-update detection, premium / private feeds, episode size cap, archive, custom playlists, queue / badge / history sync, RTL + CJK + initial localizations (es / pt / ja / de / fr), language indicator, quiet hours, download-complete notifs, recent / saved searches, clip creation + share, data export, reset settings, diagnostics export, Mac Catalyst, external display, large widgets, action button shortcut presets.

**Differentiating.** SwiftData lands empty (architecture §6). Wake-word "Hey, podcast" (UX-06). Briefing scheduling (UX-08 + UX-14 handoff). Voice persona swap post-S5. Threading evolution view for guests (≥3 chronological mentions). Speaker-stance evolution `Hear in context` affordance. Per-tier per-tool override UI for Nostr friends. iCloud-Drive Obsidian mirror for the wiki. Shared subscription via `pcst://` deep link or Nostr event. Co-listen via SharePlay (deferred to v2 if SharePlay budget tight).

### v2 — After product-market fit

Smart playlists, alt feeds, Apple Podcasts Subscriptions OAuth, translated transcripts, SharePlay co-listen, V4V Lightning tipping, boostagrams, in-app tip jar, Spatial Audio, Family Sharing, community / comments. Watch standalone RAG. CarPlay full voice-conversational template (CPListItem + voice-primary modality). Quote-reply visual link in chat. WhisperKit Pro toggle for languages where Apple's accuracy lags.

---

## 11. Risk Register

| # | Risk | Likelihood / impact | Mitigation | Owner |
|---|---|---|---|---|
| 1 | **Hallucination at synthesis** (wiki, briefings, threading misattribute claims to a host who didn't say them) | High / Severe | Provenance-or-it-doesn't-render. Post-compile verifier. Diarization-confidence threshold below which we attribute to "the show," not a named speaker. | Knowledge / Briefing |
| 2 | **Voice barge-in false-positive** (orb interrupts itself for coughs, laughs in podcast audio) | Medium / Severe | AEC always on. 250 ms voiced minimum. Cross-correlate against TTS ring buffer. Optimistic preview rim-light with forgiveness behavior. | Voice |
| 3 | **AVAudioSession routing mistakes** (player talks over voice, voice talks over briefing) | Medium / Severe | Single `AudioSessionCoordinator`. State machine with pre-warm on wake gesture. Integration tests with fake `AVAudioSession`. | Audio |
| 4 | **Transcription latency vs UX promise** ("talk to all your podcasts" with no transcripts ready) | Medium / High | Background prefetch from moment of subscription. *Transcribing… 38 %* surfaced honestly. On-device `SpeechAnalyzer` opt-in for power users who want it now. | Transcript |
| 5 | **Speaker identity misresolution** ("Tim said X" when guest said X) | High / High | Tiered resolver (RSS → NER → voiceprint → user disambig). Attribute to "the show" below threshold. Disambiguation chooser at page open. | Knowledge / UX-13 |
| 6 | **OpenRouter embedding model availability** (coverage thinner than chat) | Medium / Medium | **Verify before wiring.** `EmbeddingProvider` protocol abstracts; fallback to direct Voyage / Cohere / OpenAI keys. | Knowledge |
| 7 | **Live Activity battery drain** (transcript-line scrolling at high frequency) | Medium / High | Throttle to one update per transcript-segment boundary (~6–10 s), not per word. Battery soak test before ship. | Ambient |
| 8 | **Trial-budget abuse** (multi-install farming during onboarding) | Medium / Medium | Device-attested + capped at one briefing + ~2 K agent tokens. Finance sign-off on per-user ceiling. | Onboarding |
| 9 | **Photo / clip licensing posture** (scraped portraits, cross-publisher clip embedding) | Medium / High (legal) | Wikimedia/Commons → verified social → monogram. Quote ceiling 125 char. Fair-use ≤20 s clip ceiling for inline cross-publisher. Legal review before ship. | UX-13 + UX-09 |
| 10 | **Nostr DM tool-exposure (privilege escalation)** | Low / Severe | Tier-gated tool exposure (Reader / Suggester / Actor). Per-tool overrides. TTL on agent-originated DMs; max hop count of 2. | Nostr |

---

## 12. Appendix — Source Briefs Index

### Project context

- [docs/spec/PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) — vision, foundations, marquee user stories.
- [docs/spec/baseline-podcast-features.md](baseline-podcast-features.md) — table-stakes feature checklist.

### UX briefs (15)

- [docs/spec/briefs/ux-01-now-playing.md](briefs/ux-01-now-playing.md) — Now Playing (the hero surface).
- [docs/spec/briefs/ux-02-library.md](briefs/ux-02-library.md) — Library & Subscriptions.
- [docs/spec/briefs/ux-03-episode-detail.md](briefs/ux-03-episode-detail.md) — Episode Detail & Transcript Reader.
- [docs/spec/briefs/ux-04-llm-wiki.md](briefs/ux-04-llm-wiki.md) — LLM Wiki Browser.
- [docs/spec/briefs/ux-05-agent-chat.md](briefs/ux-05-agent-chat.md) — Agent Chat (text mode).
- [docs/spec/briefs/ux-06-voice-mode.md](briefs/ux-06-voice-mode.md) — Voice Conversational Mode.
- [docs/spec/briefs/ux-07-search-discovery.md](briefs/ux-07-search-discovery.md) — Semantic Search & Discovery.
- [docs/spec/briefs/ux-08-briefings-tldr.md](briefs/ux-08-briefings-tldr.md) — AI Briefings / TLDR Player.
- [docs/spec/briefs/ux-09-cross-episode-threading.md](briefs/ux-09-cross-episode-threading.md) — Cross-Episode Knowledge Threading.
- [docs/spec/briefs/ux-10-onboarding.md](briefs/ux-10-onboarding.md) — Onboarding & First Run.
- [docs/spec/briefs/ux-11-ambient-surfaces.md](briefs/ux-11-ambient-surfaces.md) — Ambient Surfaces (Lock Screen / Live Activities / Widgets / CarPlay / Watch / AirPods / Action Button / Shortcuts).
- [docs/spec/briefs/ux-12-nostr-communication.md](briefs/ux-12-nostr-communication.md) — Nostr Communication.
- [docs/spec/briefs/ux-13-speaker-topic-profiles.md](briefs/ux-13-speaker-topic-profiles.md) — Speaker & Topic Profiles.
- [docs/spec/briefs/ux-14-proactive-agent-notifications.md](briefs/ux-14-proactive-agent-notifications.md) — Proactive Agent & Notifications.
- [docs/spec/briefs/ux-15-liquid-glass-system.md](briefs/ux-15-liquid-glass-system.md) — Liquid Glass Design System & Motion Language.

### Research notes (6)

- [docs/spec/research/snipd-feature-model.md](research/snipd-feature-model.md) — Snipd feature model for headphone snipping, mentioned books, guest graph, auto-chapters, and AI DJ-style routes.
- [docs/spec/research/template-architecture-and-extension-plan.md](research/template-architecture-and-extension-plan.md) — what we have, what we extend, what we add. The architectural backbone.
- [docs/spec/research/skeleton-bootstrap-report.md](research/skeleton-bootstrap-report.md) — the rename pass and module/feature stubs already on disk.
- [docs/spec/research/transcription-stack.md](research/transcription-stack.md) — Scribe + publisher + iOS-26 SpeechAnalyzer pipeline.
- [docs/spec/research/embeddings-rag-stack.md](research/embeddings-rag-stack.md) — OpenRouter embeddings + sqlite-vec + FTS5 + RRF + reranker.
- [docs/spec/research/voice-stt-tts-stack.md](research/voice-stt-tts-stack.md) — STT + TTS + AVAudioSession + barge-in.
- [docs/spec/research/llm-wiki-deep-dive.md](research/llm-wiki-deep-dive.md) — adapting nvk/llm-wiki to a podcast knowledge base.

---

*End of spec. The next deliverable is an engineering plan that turns §7 into PRs and §10 into a sprint cadence.*
