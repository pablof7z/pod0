# Pod0 product surface narrowing exploration

Date: 2026-07-17
Project/context: pod0 (Podcastr iOS app) — catalog current feature surface, decide keep/delete before architecture work expands
Status: decided

## Core Question

- What is the product at its core, and which of the currently-built features should be kept vs deleted (delete = remove for now, possibly reintroduce later)?

## Current Working Model

- The core is the agent: an agent-first podcast app. Everything is evaluated by how much it serves the agent experience.
- Sequencing decision driver: narrow the surface FIRST, then apply the durability architecture work ([[2026-07-17-durable-workflow-architecture-exploration]]) to the smaller surface — no point making wiki/triage pipelines durable if they're deleted.

## Observations

- User-declared KEEP (fixed): agent stuff; proactive listening of the agent; chat with the agent; agent RAG-backed search for episodes; TTS/STT; podcast generation by the agent; chapter generation; transcriptions; ad-detection/skipping.
- User-declared DELETE (fixed): wiki/briefs; triage ("we'll bring it back soon, but it sucks right now").
- Codebase surface (from 2026-07-17 survey): Features/{Settings 80, Player 42, Home 24, Library 19, Agent 19, EpisodeDetail 18, Feedback 14, Identity 13, Onboarding 11, Wiki 8, Briefings 7, Search 5, Threading 3, Friends 3, Voice 2, Clippings 2, AgentChat, WhatsNew, Bookmarks} plus top-level {Audio, Briefing, Voice, Knowledge, Podcast, Transcript, Agent, Services, CarPlay, AppIntents, Widget, Design, Domain, State}; 583 source files.

## Constraints And Invariants

- "Delete" means delete-for-now; reintroduction later should stay cheap (git history preserves code; avoid leaving load-bearing stubs).
- Keep-for-sure features must not break when delete-candidates are removed — blast-radius mapping required before any deletion.

## Preferences

- User wants to zero in on the core product identity before expanding architecture.

## Assumptions

- "Podcast generation by the agent" maps to the OwnedPodcasts agent-tool family — to verify via swarm.
- "Proactive listening" maps to agent picks / scheduled agent runs / proactive surfaces — to verify via swarm.

## Open Questions

- RESOLVED (cat-agent-core): briefs ≠ podcast generation. Briefings = always-on `generate_briefing` tool inside `AgentTools+Podcast.swift:~307-336` + `LiveBriefingComposerAdapter.swift` + Features/Briefings UI — surgically deletable. Podcast generation = `podcast_generation` skill (`PodcastGenerationSkill.swift`) + `AgentTools+TTS.swift` + `AgentTTSComposer.swift` (483 LOC stitching engine) + OwnedPodcasts layer (`AgentTools+OwnedPodcasts.swift`, `LiveAgentOwnedPodcastManager.swift`: create/update/delete shows, artwork gen, NIP-74 Nostr publish) — most production-complete feature in the agent system. CAVEAT before deleting briefings: `AgentTTSComposer.swift` has a "Briefing" grep hit — verify briefing audio doesn't share the TTS-composer path before wholesale deletion.
- RESOLVED (cat-agent-core): "proactive listening" has no single implementation. Agent Picks (`AgentPicksService`, mature, tested, fallback path) = proactive curation on Home; Scheduled Agent Tasks = headless agent runs but fire only on app open/foreground (no BGTaskScheduler). True background proactivity must be built — scaffolding exists, OS trigger doesn't.
- NEW gray-zone from cat-agent-core: Nostr peer-agent conversations (`AgentRelayBridge`, `AgentTools+PeerActions`, `NostrAgentSurface`, ask-consent flow) — complete but large social/multi-agent surface; not on either fixed list. `PerplexityClient`/`perplexity_search` — unclassified. Conversation-history skill and YouTube-ingestion skill — small, self-contained, feed kept features.
- Gray-zone features awaiting verdict: the nostr trio (user identity/login+comments; agent-to-agent Friends messaging; NIP-74 discovery/publish — note vision doc lists nostr agent commands as marquee, but cat-social recommends full cluster deletion under minimal-surface lens; severable either way), CarPlay (strongest periphery delete candidate), Clippings+Bookmarks consolidation, Widget, sleep timer, Share Quote, Feedback, WhatsNew, Onboarding scope, Settings breadth, PerplexityClient. Threading resolved → wiki bucket.
- Does deleting triage require restoring a pre-triage new-episode flow (notifications/inbox placement)?

## Hypotheses

- Deleting wiki/briefings/triage removes three of the worst durability offenders found in the architecture audit, shrinking the durability epic materially.

## Risks

- Hidden coupling: agent tools/skills may reference wiki/briefings/triage surfaces; deleting could break agent tool dispatch. CONFIRMED instances: always-on `query_wiki` in main dispatcher/schema; `generate_briefing` embedded in `AgentTools+Podcast.swift`; `AgentTTSComposer` (keep) uses `BriefingAudioStitcher`/`BriefingTrack` (delete lane).
- Home/EpisodeDetail UI may embed triage/wiki affordances whose removal touches keep-features' views.
- Keep-list item "voice conversation with agent" is currently a stub-wired demo (no agent bridge, no audio-session coordination) — keeping it means committing to an integration build, not just not-deleting it.

## Evidence Gathered

- cat-knowledge report (2026-07-17): All 4 STT providers (ElevenLabs Scribe, AssemblyAI, OpenRouter Whisper, Apple on-device) fully wired via `TranscriptIngestService`, each a distinct cost/privacy tradeoff — no redundancy to prune. Publisher transcript ingestion (Podcasting 2.0 JSON/VTT/SRT) is the preferred path. RAG stack (ChunkBuilder → sqlite-vec+fts5 VectorIndex with RRF hybrid retrieval → optional Cohere rerank) is production-grade and consumed by agent tools + skills incl. `PodcastGenerationSkill`, `ConversationHistorySkill`, `YouTubeIngestionSkill`. AIChapterCompiler = chapters + ad-detection + per-chapter summaries in one LLM call (old `AdSegmentDetector` already merged in; only its test file name survives). Ad-skip consumption in `PlaybackState+AdSkip` is wired and gated on settings. Dead code: `TranscriptionQueue.swift` (delete + 3 stale doc-comments) and `InMemoryVectorStore.swift` (test-only, misleading fallback comment).
  - LOAD-BEARING SURPRISE: PodcastCategorization gates transcription — `effectiveTranscriptionEnabled(forPodcast:)` is a hard per-category opt-out check inside `TranscriptIngestService.ingest()`. Keep it (243 LOC) or unwind the gate.
  - Wiki deletion coordination: wiki files are interleaved inside `App/Sources/Knowledge/` (WikiGenerator/Verifier/Storage/Triggers/etc. + `RAGService.wikiRAG` adapter + STEP 4 of `persistAndIndex`); excision must strip those without touching shared RAG infra (VectorIndex/RAGSearch/Chunk/embedders).
  - Signal for briefs-vs-podcast-generation question: a `PodcastGenerationSkill` exists in Agent/Skills, separate from Features/Briefings — likely the keep-listed "podcast generation."
- cat-agent-core report (2026-07-17): Agent system = 49 files/~9.8k LOC in Agent/ + 19 in Features/Agent. Full tool census taken (~50 tools; 4 skills: podcast_generation, wiki_research, conversation_history, youtube_ingestion). Wiki tooling: `query_wiki` is ALWAYS-ON (dispatcher+schema touch on delete); create/list/delete wiki gated behind wiki_research skill. Triage has zero footprint in agent tools (lives in Services/InboxTriagePrompt). Notes/memory + AgentMemoryCompiler feed every agent surface's system prompt — foundational. `AgentTools+Podcast.swift` at 468/500 hard line limit; deleting `generate_briefing`+`query_wiki` cases conveniently shrinks it. Unclassified follow-ups: `PodcastAgentToolValues.swift` (628 LOC) and `AgentTools+PodcastInventory.swift` wiki/briefing entanglement not read in depth.

- cat-voice report (2026-07-17): FIVE voice capabilities. (1) Voice conversation with agent (`AudioConversationManager` + `VoiceView`/orb + `BargeInDetector`): UI/state machine well-built BUT NOT WIRED — production uses `StubVoiceTurnDelegate` (canned echo, no LLM) and `NoopAudioSessionCoordinator` (no ducking/capture mode); `AudioSessionCoordinator.shared.voiceClient` is never assigned; ElevenLabs WS envelope unverified per author comment; SpeechDetector stub always-true; zero tests. Keep = budget a real integration task. (2) `generate_tts_episode` engine (`AgentTTSComposer`, 443-483 LOC): solid, working, BUT depends on `BriefingAudioStitcher`/`BriefingTrack` from the delete-listed briefing lane — extract stitcher as shared audio util before briefing deletion. (3) ElevenLabs voice preview in Settings: small, working, keep. (4) Voice notes on episodes (`VoiceNoteRealtimeSTT`, 585 LOC, ElevenLabs realtime scribe): most complete voice feature, real bridge into agent chat via `.askAgentRequested`; duplicates STT infra outside Voice/ — consolidation candidate later. (5) `RationaleNarrator` (Home pick rationale readout): depends on `AVSpeechFallback` from Voice/ — fate tied to Agent Picks keep. CarPlay has no voice integration.

- cat-playback report (2026-07-17): 20 features cataloged across Audio/, Features/Player (42 files), CarPlay, Widget, AppIntents, Clippings, Bookmarks (~7.3k LOC). Agent-coupled keeps hiding in "playback": Up Next queue segment-bounds (agent `play_episode` primitive), chapter rail ask-agent, AutoSnip clip capture (LLM boundary refinement via `ClipBoundaryResolver`), voice notes, clip/generation source chips, `StartVoiceModeIntent` (Action Button/Siri → voice mode; misfiled under AppIntents). DELETE: `PlayerTranscriptScrollView.swift` + `+AskAgent.swift` — verified dead, zero call sites (keep `AskAgentDispatcher` notification constants if referenced elsewhere). STRONGEST DELETE CANDIDATE: CarPlay (7 files ~700 LOC, fully built, zero agent coupling, pure parity surface). DEFER: sleep timer, widget, plain share links. VERIFY: seek-history `jumpBack()` has no visible UI trigger — speculative infra? CONSOLIDATE: Bookmarks vs Clippings (two top-level screens over overlapping Clip/notes data → one "Saved" IA). Share Quote duplicates AutoSnip's LLM path — merge candidate.

- cat-social report (2026-07-17): nostr = THREE independent clusters, ~9-10k LOC total, all cleanly severable from agent core (nostr layers reuse `AgentChatSession`/`AgentTools` engine; no reverse dependency). (1) User identity/login (Identity 13 files + Nip46 9 files, ~3k LOC incl. NIP-46 remote signer) — sole real consumer is episode comments (3 files, 1 call site in `EpisodeDetailHeroView:58`); Nip46 also serves `Feedback/Nip46ConnectCard` (dev feedback channel — separate disposition needed). (2) Agent identity + Friends + agent-to-agent messaging (~35 files, 4.5-5k LOC: `AgentRelayBridge`, `NostrAgentResponder`, `NostrRelayService`, Friends UI, access control, conversations) — mature, not stub. Structural leaks into core: 8 nostr fields in `AppState`, `send_friend_message` in the ALWAYS-ON core tool schema, non-optional `peerPublisher`/`friendDirectory` in `PodcastAgentToolDeps`, `Podcast.nostrVisibility` DEFAULTS TO `.public` (agent shows auto-publish to nostr today). All four = small mechanical edits. (3) NIP-74 podcast discovery in Add Show (~640 LOC) — only useful if agents publish shows to nostr. OwnedPodcasts/podcast-generation works fully with nostr disabled — publish is an optional gated side path. `Features/Settings/Agent/` is a MIXED hub (keep-infra views interleaved with nostr views — cherry-pick, never delete wholesale). `Features/Threading` = NOT nostr; wiki-adjacent topic inference ("lives behind the wiki surface") → route through wiki deletion. No dead code in the cluster.

- cat-deletions report (2026-07-17): full blast-radius map for wiki/briefings/triage. Inventory: wiki 16 files ~2.6k LOC (+agent-side files: AgentTools+PodcastWiki, LiveWikiStorageAdapter, WikiResearchSkill, PodcastAgentToolValues+Wiki), briefings 20 files ~3.9k LOC, triage 7 files ~0.8k LOC. All Episode/Settings field deletions decode-safe (decodeIfPresent; Episode persisted as JSON blob in SQLite → zero schema migration). HIGH risks before deleting: (1) `WikiOpenRouterClient.swift` is a GENERIC OpenRouter client used by KEPT AIChapterCompiler:109, ClipBoundaryResolver:61+, PodcastCategorizationService:112, AgentChatTitleGenerator:45 — rename/extract first; (2) Threading orphan: `ThreadingTopicListView`'s ONLY entry point is WikiView's "Threads" button — rehome or delete 3 files/853 LOC as collateral (user decision); (3) `WikiPage.normalize(slug:)` used by `AppStateStore+Threading:34-37` — relocate; (4) `Settings.wikiModel/wikiModelName` reused by AutoSnip + PlayerShareSheet as generic utility model — keep fields, relabel picker; (5) wiki+briefing deps interleaved on same `PodcastAgentToolDeps` constructor lines — one coordinated edit. MEDIUM: Home featured section product decision — Inbox section either removed or reverted to pre-Inbox AgentPicksService curation (comments say Inbox replaced it); SubscriptionRefreshService gate removal (mechanical: dispatch side effects immediately post-upsert, delete waitForTriageToSettle — kills the 60s stall, synergy with durability epic); `BriefingsView` ALREADY ORPHANED in nav (zero call sites — only BriefingComposeSheet reachable via HomeThreadedTodayView). Voice enum `.duckedWhileBriefing` case in VoiceView must be removed with care. Tests: WikiStorageMigrationTests + WikiVerifyTests delete wholesale; surgical strips in AgentSkillsTests/AgentToolsPodcastSearchTests/PodcastSearchTests/shared mocks; zero triage tests exist. Wiki pages on disk (`Application Support/podcastr/wiki/`) left orphaned unless one-time cleanup added.

- cat-library report (2026-07-17): Library (19 files/4k LOC) fully clean of wiki/triage — all KEEP. Home is the heaviest triage entanglement (6 hard-coupled files; `HomeFeaturedSection`'s whole premise is Inbox; card/shimmer shells salvageable for AgentPicks). Search treats wiki as a first-class 4th domain — 4 of 5 files need surgical edits. EpisodeDetail: zero wiki/triage coupling but ~31% dead (orphaned clip composer 685 LOC, ClipVideoComposer always throws .notImplemented 363 LOC, ChapterRailView 84 LOC). Settings 80 files/13.9k LOC: wiki/briefing touch only ~7 files lightly; real bloat = `AI/Usage` dev cost dashboard (731 LOC, cut/gate candidate), OpenRouter model browser (1.9k LOC, 3x richer than needed), 5x duplicated provider-connection screens (~1050 LOC, refactor to one generic view), dead pass-through wrappers. Onboarding: `OnboardingElevenLabsPage` dead (110 LOC); ReadyPage has one wiki chip. Feedback: TWO parallel systems — live shake→annotate path (~323 LOC, works but dead-ends at .composing) and a NEVER-INSTANTIATED nostr feedback-thread system (~2031 LOC = 79% dead); `Nip46ConnectCard` misplaced but used by Identity (move not delete). WhatsNew: keep unconditionally. DEAD-CODE TOTAL (independent of narrowing): ~3,665 LOC + TranscriptionQueue 187 + PlayerTranscriptScrollView + InMemoryVectorStore. Misc bugs: HomeView dead `voiceOverDetailRoute` state; `AgentConnectionSettingsView` possibly orphaned.

## Adjacent Checks

- (pending)

## Alternatives Considered

- Feature-flagging instead of deleting: not yet discussed with user; deletion stated as the intent.

## Rejected Options

- (none yet)

## Decisions Or Emerging Direction

- Decided (user): keep agent core (chat, proactive, RAG search, TTS/STT, podcast generation) plus chapter generation, transcriptions, ad-detection/skipping; delete wiki/briefs and triage for now.
- Decided (user, 2026-07-17 PM): begin deletion of the decided cuts immediately via background agent while remaining decisions pend; delete ALL verified dead code; delete the ENTIRE Feedback feature (live shake path included).
- Decided (user, 2026-07-17 PM, round 2): DELETE Threading topic-browse UI entirely (ThreadingTopicListView/TopicView/MentionRow, ~853 LOC; ThreadingInferenceService + Threaded Today stay); DELETE ALL nostr (all three clusters: user identity/Nip46/comments, agent-to-agent Friends messaging, NIP-74 publish + discovery; `Nip46ConnectCard` now deleted too — supersedes the move-to-Identity plan). KEEP Perplexity, YouTube ingestion, and the Now Playing widget.
- Decided (user, 2026-07-17 PM, via TTS ask): DELETE CarPlay; Home featured = AgentPicksService curation (interim confirmed as final); cleanup trims IN: merge Bookmarks into Clippings (one Saved surface), merge Share Quote into AutoSnip path, unify the 5 provider-connect screens into one generic view. Cleanup trim OUT (kept): LLM usage dashboard (AI/Usage). No product decisions remain open — surface narrowing is fully decided.
- Interim (agent-chosen, reversible, flagged to user): Home featured section reverts to AgentPicksService-backed curation during triage removal (pending user's final call on decision 3); Threading topic-browse UI left in place but orphaned pending decision 4.
- Emerging (agent-proposed post-swarm, 2026-07-17, NOT yet user-approved):
  - Delete unconditionally: all verified dead code (~4k LOC incl. dead Feedback path, clip composer, TranscriptionQueue, PlayerTranscriptScrollView, HomeResumeCard/Dateline/FilterChips, OnboardingElevenLabsPage, InMemoryVectorStore).
  - Deletion prerequisites for wiki/briefs/triage: rename `WikiOpenRouterClient`→generic OpenRouter client; relocate `WikiPage.normalize(slug:)`; extract `BriefingAudioStitcher` for AgentTTSComposer; relabel `wikiModel` as utility model; coordinated `PodcastAgentToolDeps` edit; Home featured section reverts to AgentPicks curation (recommendation).
  - Decision frontier for user: nostr trio (identity+comments / friends+agent-to-agent / NIP-74 publish+discovery), CarPlay, Threading topic-browse UI collateral (853 LOC), Feedback live shake path, Perplexity, Bookmarks↔Clippings consolidation, Settings Usage dashboard.
  - Keep-but-build: voice conversation mode needs real integration (stub delegate + noop audio coordinator today); scheduled tasks need BGTaskScheduler for true proactivity.

## Follow-Up Artifacts

- EXECUTION COMPLETE 2026-07-17: branch `surface-narrowing`, 14 commits ahead of origin/master, NOT pushed. Final tally: 322 files changed, +2,002/−33,104 (−31,102 net). Full test suite: 514 tests, 0 failures; clean build. All phases done: wiki, briefings, triage, dead code, Feedback, Threading topic browse, full nostr (all 3 clusters), CarPlay, Saved-screen consolidation, Share Quote→AutoSnip merge, provider-screen unification, changelog entries. Phases 10b/10c/11 finished by team-lead directly after the cutter agent stalled. Next: review/PR/merge, then the durability epic ([[2026-07-17-durable-workflow-architecture-exploration]]) targets this surface.
- Original phase 0-6 record: branch `surface-narrowing` (worktree `.claude/worktrees/surface-narrowing`, not pushed). Phases 0-6 (wiki/briefings/triage/dead-code/Feedback) COMPLETE 2026-07-17: 181 files, -14,052 net LOC, build green, 604/605 tests (1 pre-existing NMP relay failure, deleted in phase 8). Cutter caught audit misses: SilentAudioWriter in BriefingFakes (would have broken AgentTTSComposer), stranded Notification.Name extension, BriefingRAGSearchAdapter/RAGService.briefingRAG, AudioSessionCoordinator.briefingPlayback scaffolding. Feedback deletion had already landed upstream (master carries 28 NMP/identity commits — note: NMP work exists on master; owner's delete-all-nostr instruction stands). Phases 7-10 (Threading browse, full nostr, CarPlay, consolidations) in progress.
