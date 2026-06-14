---
title: Podcast TUI
slug: podcast-tui
topic: podcast-tui
summary: The podcast TUI is a ratatui-based terminal player located in apps/podcast-tui/ that leverages the components and codebase of ../nostr-multi-platform
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-28
updated: 2026-06-13
verified: 2026-05-28
compiled-from: conversation
sources:
  - session:1a2f2460-74e7-4309-9dcc-99d19936c123
  - session:f1804b3d-52ea-4a3f-bbf2-608cef7c7468
  - session:31d36c85-992e-43d0-a31c-ab1c8e43344c
  - session:a6320d4d-f2c8-4a8b-a21a-d71f5af73509
  - session:8eb3f00f-b245-4f03-80f0-15151d9aba28
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Podcast TUI

## Overview

The podcast TUI is a ratatui-based terminal player located in apps/podcast-tui/ that leverages the components and codebase of ../nostr-multi-platform. All business logic (subscriptions, playback state, search parsing, queue management) lives entirely in nmp-app-podcast; the TUI is a thin NMP shell that dispatches actions and renders snapshots with zero business logic duplication. The TUI accepts an optional --data-dir <path> flag to set a persistent data directory for the kernel's store. <!-- [^1a2f2-1] -->

## Architecture

The TUI boots the full NMP + Podcast kernel via nmp-app-podcast and installs a native audio capability host. The TUI executor obtains the `PodcastHandle` via a `OnceLock<usize>` static (mirroring the existing `AUDIO_HOST` pattern), set in `AppRuntime::new` before `nmp_app_start`. The TUI does NOT use the NmpUpdateBridge, which corrupts FlatBuffer binary frames by treating them as UTF-8 JSON strings; instead, it receives kernel state updates via snapshot polling using `nmp_app_podcast_snapshot` / `nmp_app_podcast_snapshot_rev`. The 250 ms tick handler polls the podcast snapshot when `rev` changes and applies the JSON directly to state via `app.rs`'s `apply_snapshot_json` method, which also silences the noisy bridge parse-error spam since FlatBuffer frames are expected. The `podcast-feeds` dependency is required in the podcast-tui Cargo.toml for the `HttpRequest`/`HttpResult` types in the capability handler. Episode deserialization uses native Rust types via `PodcastUpdate` into TUI-local `PodcastRow`/`EpisodeRow` structs (which do not derive `Eq` because they contain `f64` fields), not `podcast_core::EpisodeSummary`. The TUI includes a `reqwest::blocking` HTTP executor in the capability callback to handle `nmp.http.capability` requests (e.g., iTunes search, RSS subscribe); the async HTTP handler extracts the transport body into a shared `run_http` helper so the sync and async paths use identical transport logic. All podcast kernel actions sent from the TUI use the flat JSON format `{"op":"snake_case", ...fields}` (e.g., `{"op":"subscribe", "feed_url":"..."}`), not the variant-wrapped format `{"VariantName":{...}}`. The `subscribe` and `search_itunes` actions are sent to the namespace `"podcast"`, not `"podcast.podcast"`. The `download`, `star`, and `unstar` actions are sent to the namespace `"podcast"`, not `"podcast.player"`. The `mark_played` and `mark_unplayed` actions are dispatched as `mark_listened` and `mark_unlistened` respectively to the namespace `"podcast.inbox"`, not `"podcast.player"`. The `add_to_queue` and `remove_from_queue` actions are dispatched as `add_last` and `remove` respectively, not `QueueAdd` and `QueueRemove`. The `subscribe` and `search` actions in `input.rs` surface errors in the status bar rather than silently discarding them with `let _ = ...`. A headless integration test binary at apps/podcast-tui/src/bin/integration_test.rs boots the real kernel against a live RSS feed and asserts 7 sequential behaviors: subscribe, episodes appear, queue add, queue remove, mark played, mark unplayed, and speed setting. FFI-DTO removals must grep the entire workspace (including podcast-tui), not just apps/nmp-app-podcast, because the TUI binds the same PodcastUpdate and projection structs by path dependency. The flat agent_notes wire field and AgentNoteSummary DTO were retired (PR #435/#437) because conversations carry the data; the podcast-tui crate was a missed live consumer that required migration to NostrConversationDTO.

<!-- citations: [^1a2f2-2] [^f1804-1] [^31d36-1] [^a6320-3] [^8eb3f-11] [^c1691-303] [^c1691-320] -->
## Audio Playback

Audio playback detects if mpv is available and drives playback through mpv's JSON IPC socket (--input-ipc-server), falling back to a stub mode on systems without mpv. <!-- [^1a2f2-3] -->

## UI Layout

The UI layout consists of three rows — tab bar, main body, and player bar — plus a status line. The TUI has four tabs: Library, Queue, Search, and Settings. The Library view is a split pane with a podcast list on the left and an episode list on the right, showing unplayed counts, playback progress, and duration. The player bar displays the now-playing title, play/pause indicator, and a progress gauge. <!-- [^1a2f2-4] -->

## Key Bindings

The / and n key handlers work from any tab (Library, Queue, Search, Settings) in Normal mode via the global key dispatcher, not just on the Library tab. Pressing Enter on a search input submits the search and auto-switches to the Search tab. Keyboard controls include: Tab/Shift+Tab to switch tabs; h/l or arrows to switch panes; j/k or arrows to navigate lists; Enter to play selected episode from current position; Space to pause/resume; p to play from start; d to download episode; s/S to star/unstar; a to add to queue; n to subscribe to RSS feed (opens input prompt); / to search iTunes (opens input prompt); ? for help overlay; q/Ctrl+C to quit. <!-- [^1a2f2-5] -->

## Subscriptions

Pressing Enter or s on any search result dispatches podcast.subscribe with the feed_url; the NMP kernel's actor thread handles the RSS fetch, parse, and store update asynchronously, and the TUI sees the new podcast appear in the Library tab on the next snapshot tick. <!-- [^1a2f2-6] -->

## Downloads

AppState parses downloads.active from the kernel snapshot to display download progress. A compact ↓ 3 45% indicator appears in the status bar when downloads are active (green, bold). When a download drops out of the active list (completed), the TUI pushes a brief toast notification. All download logic (HTTP fetch, progress tracking, file writes) stays in nmp-app-podcast; the TUI is pure render for downloads. <!-- [^1a2f2-7] -->

## Out of Scope

The TUI does not include: subscribe-from-search (was view-only, now fixed), download progress display (now fixed), chapter navigation, transcript view, Nostr features (comments, social graph, publish), agent/chat/AI picks/tts/voice, bookmarks/clips, settings editing, actual playback position reporting back to the kernel (position drifts in stub mode), image rendering (ratatui can't do cover art), queue tab selection cursor, sleep timer, auto-skip ads, or speed/volume UI controls. <!-- [^1a2f2-8] -->


Android Tier-2 Inbox and Transcripts surfaces are implemented on the shared kernel (InboxAction::{Triage,Dismiss,MarkListened} and FetchTranscript routing verified against the Rust namespace router). <!-- [^c1691-34] -->
## Functional Behavior

iTunes search returns results (e.g. 25 podcasts for 'technology') and RSS subscribe populates the library with episodes from the feed. <!-- [^31d36-2] -->

## Known Issues

The current state of the `apps/podcast-tui/` codebase has diverged and no longer compiles, containing at least 8 compilation errors including `main.rs` calling a nonexistent `state.apply_podcast_update(update)` method, an unhandled `Tab::Inbox` variant, and various other issues. <!-- [^31d36-3] -->
