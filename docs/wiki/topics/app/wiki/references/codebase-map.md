---
title: "Codebase Map"
summary: "Where to look before changing each major Podcastr subsystem."
tags: [implementation, swift, files, codebase]
aliases: [Podcastr file map, implementation map]
sources:
  - raw/notes/2026-05-12-app-system-source-map.md
created: 2026-05-12
updated: 2026-05-12
verified: 2026-05-12
volatility: warm
confidence: high
---

# Codebase Map

Use this map before editing. Podcastr has many connected surfaces, and the fastest way to avoid drift is to start in the owner module instead of patching the visible view only.

## Project And App Shell

- `Project.swift`: Tuist targets, bundle IDs, App Group, deployment target, packages, schemes.
- `App/Sources/AppMain.swift`: app entry point.
- `App/Sources/App/RootView.swift`: root tabs, toolbar sheets, mini-player, onboarding, deep links, feedback, voice mode, Spotlight.
- `App/Sources/App/AppDelegate.swift`: app lifecycle, shortcuts, launch routing.

## Domain And State

- `App/Sources/Domain/`: models such as `AppState`, `Settings`, `Episode`, `PodcastSubscription`, `Clip`, `Friend`, `AgentMemory`, `ThreadingTopic`, and related value types.
- `App/Sources/State/AppStateStore.swift` plus extensions: mutations, projections, persistence side effects, iCloud settings sync, episodes, categories, clips, notes, Nostr state, agent activity.
- `App/Sources/State/Persistence.swift`: App Group state persistence.
- `App/Sources/State/EpisodeSQLiteStore.swift`: large episode storage support.

## Podcast And Playback

- `App/Sources/Podcast/`: RSS parsing, feed client, OPML, categories, download state, transcript state.
- `App/Sources/Audio/`: audio engine, Now Playing center, audio session, sleep timer.
- `App/Sources/Features/Player/`: full player, mini-player, controls, chapters, transcript rail, share sheet, queue, sleep/speed sheets, AutoSnip, voice note recording.
- `App/Sources/Services/EpisodeDownloadService*`: downloads and auto-download policy.

## Knowledge

- `App/Sources/Transcript/`: publisher transcript parsing, STT clients, transcript queue and adapters.
- `App/Sources/Services/TranscriptIngestService*`: transcript ingestion and auto-ingest orchestration.
- `App/Sources/Knowledge/`: chunks, vector index, embeddings clients, RAG, wiki generator, wiki verifier, wiki page model, citations.
- `App/Sources/Services/RAGService*`: singleton retrieval stack and adapters.
- `App/Sources/Features/Wiki/`: user-facing wiki home, page view, generation sheet, citation UI.

## Agent, Voice, Briefings

- `App/Sources/Agent/`: prompt, tool schema, podcast tools, tool values/deps, live adapters, Perplexity, delegation, run logging.
- `App/Sources/Features/Agent/` and `App/Sources/Features/AgentChat/`: chat session, UI, conversation history, transcript export, activity sheets.
- `App/Sources/Voice/` and `App/Sources/Features/Voice/`: voice conversation, STT, TTS, barge-in, captions, orb surface.
- `App/Sources/Briefing/` and `App/Sources/Features/Briefings/`: generated briefing scripts, storage, audio stitching, player, composer UI.

## Settings, Providers, Identity, Feedback

- `App/Sources/Features/Settings/`: settings shell, downloads/storage/playback/transcripts/wiki/notification/category screens.
- `App/Sources/Features/Settings/AI/`: provider connection, model selection, catalog services, voice browser, usage cost screens.
- `App/Sources/Services/BYOK*` and provider credential stores: BYOK auth, token import, Keychain storage.
- `App/Sources/Features/Settings/Agent/`, `App/Sources/Features/Identity/`, `App/Sources/Services/Nip46/`, `App/Sources/Services/Nostr*`: identity, friends, approvals, relay, remote signing.
- `App/Sources/Features/Feedback/` and `App/Sources/Design/ShakeDetector.swift`: shake feedback, annotation, thread detail.

## Tests And Docs

- `AppTests/Sources/`: focused tests by subsystem; prefer adding a narrow test next to the touched behavior.
- `docs/spec/`: product spec, project context, UX briefs, work reports, research.
- `docs/wiki/`: repo-local wiki hub and topic wikis.
- `ci_scripts/`: CI bootstrap, tests, signing, archive/upload, cleanup.

## See Also

- [[app-operating-model|App Operating Model]] ([App Operating Model](../topics/app-operating-model.md))
- [[development-and-release-guide|Development And Release Guide]] ([Development And Release Guide](development-and-release-guide.md))
- [[implementation-map|Agent Implementation Map]] ([Agent Implementation Map](../../../agent/wiki/references/implementation-map.md))

## Sources

- [Podcastr App System Source Map](../../raw/notes/2026-05-12-app-system-source-map.md)
