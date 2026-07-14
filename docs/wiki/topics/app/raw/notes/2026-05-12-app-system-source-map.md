---
title: "Podcastr App System Source Map"
summary: "Repo-grounded source note for the app-wide Podcastr wiki, covering product docs, runtime surfaces, data flow, providers, and development rules."
tags: [podcastr, app, source-map, architecture]
ingested: 2026-05-12
source-type: repo-inspection
confidence: high
volatility: warm
verified: 2026-05-12
---

# Podcastr App System Source Map

## Primary Documents

- `README.md` describes Podcastr as an iOS podcast player centered on an embedded AI agent with knowledge of subscribed shows and episodes.
- `docs/spec/PROJECT_CONTEXT.md` defines the vision: talk to all podcasts, retrieve remembered moments, generate TLDR briefings, use voice mode, and support Nostr-mediated agent communication.
- `docs/architecture.md` documents the inherited single-store pattern: `AppState`, `AppStateStore`, SwiftUI views, direct store mutations, and App Group persistence.
- `docs/features.md` captures inherited template systems: shake-to-feedback, agent loop, friends, anchors, persistence, and CI/CD.
- `AGENTS.md` adds repo rules: user-facing iPhone commits must update `App/Resources/whats-new.json`; no serif fonts; prefer files below 300 lines and keep them under 500 lines.

## Runtime Surfaces

- `RootView` owns the main tabs: Home, Search, Clippings, and Wiki. Settings is a toolbar sheet; the agent is a toolbar sheet; the player is a persistent mini-player that expands into `PlayerView`.
- `RootView` also wires onboarding, shake-to-feedback, voice mode, Spotlight/deep links, ask-agent notifications, and open-player notifications.
- `HomeView` is the editorial library surface. It combines resume cards, agent picks, threaded-today hints, subscriptions, category filters, and pull-to-refresh.
- `PodcastSearchView`, `AddShowSheet`, and `SubscriptionService` cover search and subscription ingestion. Library/show details live under `Features/Library` and `Features/EpisodeDetail`.
- `PlayerView` renders Now Playing with artwork, chapters, transcript/agent affordances, share actions, sleep timer, speed controls, download status, AutoSnip hints, and chapter hydration.
- `WikiView` lists generated in-app wiki pages stored by `WikiStorage`, with search, generation, detail pages, citations, and a threading destination.
- `AgentChatView` presents the persistent chat session owned by `RootView`, supports history, transcript export, tool activity sheets, retry/regenerate, and context seeded from player gestures.
- `SettingsView` groups Account, Library, Listening, Intelligence, and System settings.

## State And Persistence

- `AppState` stores subscriptions, episodes, notes, friends, agent memories, categories, per-category settings, Nostr allow/block/pending lists, agent activity, clips, threading topics, and threading mentions.
- `AppStateStore` is `@MainActor` and `@Observable`. UI and agent tools mutate through it; the store handles persistence side effects, projections, widget reloads, iCloud settings sync, episode position debouncing, RAG attachment, download service attachment, subscription refresh, and background flushing.
- Episode metadata is held in app state and supported by `EpisodeSQLiteStore` for larger episode storage needs.
- Secrets are excluded from the app-state blob. Provider API keys live in Keychain-backed stores.

## Knowledge And Audio Pipeline

- `TranscriptIngestService` resolves publisher transcripts first, then falls back to the selected STT provider when enabled and keyed. Supported providers include ElevenLabs Scribe, AssemblyAI, OpenRouter Whisper, and Apple on-device STT.
- The transcript pipeline parses, chunks, embeds, indexes in `RAGService`, writes transcript JSON through `TranscriptStore`, and updates `Episode.transcriptState`.
- `RAGService` opens `vectors.sqlite` using SQLiteVec, combines vector and FTS search, routes embeddings through provider-aware clients, and exposes adapters for wiki generation and briefings.
- `WikiStorage` persists generated in-app wiki pages under Application Support with an inventory registry.
- `Briefing` modules compose generated audio briefings from transcript/wiki/RAG context.

## Agent, Voice, And Integrations

- `AgentPrompt` injects subscriptions, in-progress episodes, recent unplayed episodes, friends, notes, and memories into the system prompt. It tells the model to use tools for transcripts, wiki, semantic search, and external podcast playback.
- `AgentTools+Podcast` defines podcast-domain tools for playback, search, wiki, transcripts, briefing generation, Perplexity search, summarization, similar episodes, played state, downloads, transcription, feed refresh, navigation, delegation, inventory, clips, segment queues, generated TTS episodes, voice configuration, directory search, subscription, and external episode playback.
- BYOK provider setup uses `ASWebAuthenticationSession` in `BYOKConnectService`, PKCE against `https://byok.f7z.io`, and a `podcastr://byok` callback.
- `PodcastBYOKCredentialImporter` stores returned provider keys for OpenRouter, ElevenLabs, AssemblyAI, Ollama, and Perplexity through their Keychain-backed stores.
- Nostr identity, friends, pending approvals, relay settings, NIP-46 remote signing, feedback, and agent settings live under `Services/Nip46`, `Services/Nostr*`, `Features/Feedback`, `Features/Friends`, and `Features/Settings/Agent`.

## Build And Operating Rules

- `Project.swift` defines the Tuist project, bundle ID `io.f7z.podcast`, App Group `group.com.podcastr.app`, iOS 26 deployment target, app/widget/test targets, and dependencies on `secp256k1.swift`, `SQLiteVec`, and Kingfisher.
- CI scripts live in `ci_scripts/` and cover bootstrap, tests, signing assets, archive/upload, and cleanup.
- The app carries a widget target, uses App Group entitlements, and updates user-visible change notes in `App/Resources/whats-new.json`.
