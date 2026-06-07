# TUI Feature Parity

**Goal:** bring `apps/podcast-tui` to parity with the iOS and Android shells
where the terminal can reasonably expose the same Rust-kernel features. The
TUI remains a thin renderer/dispatcher: podcast policy and durable state stay
in `apps/nmp-app-podcast`.

## Current Baseline

The TUI already boots the real NMP podcast kernel, subscribes/searches through
the shared HTTP capability path, renders the library/search/inbox/queue/player
projections, and dispatches Rust actions for playback, downloads, queue,
bookmarks, and basic settings.

## 2026-06-06 Foundation PR

This PR expands the TUI from the original core-player surface to a parity
foundation:

- Adds tabs for Bookmarks, Clips, Agent, Wiki, Social, and richer Settings.
- Projects kernel clips, agent chat/messages, agent picks/tasks/notes, memory,
  wiki articles, comments, categories, active account, social contacts,
  configured relays, inbox triage state, and settings into `AppState`.
- Wires working kernel actions for queue selection/removal/clear, play-next,
  bookmark removal, AutoSnip/delete clips, agent send/clear/fetch notes, social
  contact refresh, wiki delete, and settings toggles.
- Splits TUI state, row mapping, runtime actions, and input handlers so all
  touched files stay under the repository line limits.

## 2026-06-06 Agent Slice

The Agent tab now behaves as a sectioned workspace:

- Chat: compose/clear against `podcast.agent`.
- Picks: select, play, queue, or play-next projected agent picks.
- Tasks: create, run-now, enable/disable, and delete via `podcast.tasks`.
- Notes: fetch inbound agent notes and publish public kind:1 notes via
  `PublishAgentNote` when an identity is available.
- Memory: remember `key=value`, forget selected facts, and clear the bag via
  `podcast.memory`.

Headless TUI integration now asserts agent chat projection, memory CRUD, and
agent task create/enable/disable/run/delete round trips through the kernel.

## 2026-06-06 Downloads Slice

The TUI now has a dedicated Downloads tab backed by the kernel download-queue
projection:

- Active, queued, paused, and failed rows render with progress, state, type,
  total byte count when available, URL, and error detail.
- Keyboard actions dispatch existing kernel download controls: pause, resume,
  cancel selected, cancel all, and delete a completed local episode file.
- Library, queue, inbox, and bookmarks rows now show active download state;
  episode-backed rows also show completed local files through `download_path`.
- Headless integration smoke-tests the runtime action routing for pause,
  resume, cancel, cancel-all, and delete without requiring a platform
  background-download executor.

The kernel does not currently project a centralized completed-download history,
so completed downloads are surfaced through episode rows rather than as a
standalone completed list.

## 2026-06-06 Episode Detail Slice

The episode detail overlay now renders the richer episode projection and
dispatches the kernel actions available today:

- Detail rows retain transcript URL/status/text/entries, chapter rows, summary,
  AI categories, ad segments, enclosure URL, file size, and download metadata.
- The overlay renders sections for summary, chapters, transcript, ad segments,
  comments, and description.
- Detail controls dispatch fetch transcript, fetch chapters, compile AI
  chapters, summarize episode, fetch/post comments, reset progress, and 15/30
  minute sleep timer arm/cancel actions.
- Comments are guarded by a TUI-side selected episode marker because the kernel
  projection exposes only the current comments slice, not the owning episode id.
- Headless integration smoke-tests the routing for the new detail actions
  without asserting network transcript/chapter fetches, LLM completion, or
  signed-in Nostr comment publishing.

Transcript fetch, chapter fetch/compile, summary generation, and comments still
depend on their existing kernel/platform/LLM/identity prerequisites; the TUI now
exposes the controls and projection state consistently.

## 2026-06-06 Settings Relay Slice

The Settings tab now has a relay-editor section backed by
`PodcastUpdate.configured_relays` and `podcast.settings` relay actions:

- `app.rs` navigation/input enums moved to `navigation.rs` to keep file sizes
  under the 500-line hard limit before adding more settings state.
- Settings now switches between `general` and `relays` sections with `h/l`.
- Relay rows render URL + role, support add via `url [role]`, remove selected
  relay, and cycle roles through `read`, `write`, `both`, `indexer`, and
  `both,indexer`.
- Headless integration asserts add, role update, and removal through the live
  configured-relays projection.

## 2026-06-07 Provider And Model Settings Slice

The Settings tab now has an editable `providers` section backed by the shared
`podcast.settings` actions:

- Role model rows cover agent initial/thinking, memory compilation, wiki,
  categorization, chapter compilation, embeddings, and image generation. Each
  accepts `model_id | display name`, including `openrouter:*`, `ollama:*`, and
  `local:*` model IDs.
- Provider credential metadata rows cover OpenRouter, Ollama, and ElevenLabs
  source/key-id/key-label/connected-at fields. Secrets are still not rendered
  or persisted by the TUI.
- Terminal-safe credential loading reads `OPENROUTER_API_KEY`,
  `OLLAMA_API_KEY`, `ELEVENLABS_API_KEY`, and `ASSEMBLYAI_API_KEY` from the
  process environment, pushes OpenRouter/Ollama keys through the existing
  in-memory-only kernel action, and reports STT key presence without exposing
  raw secrets.
- STT/TTS/local rows cover STT provider selection, STT key-presence reporting,
  OpenRouter Whisper, AssemblyAI, ElevenLabs STT/TTS, ElevenLabs voice, and the
  loaded local model hint.

## Parity Matrix

| Surface | TUI status | Notes |
|---|---|---|
| Subscribe/search/library/show episodes | Partial | Core flow works; needs OPML, feed refresh controls, empty/error polish, and terminal validation. |
| Playback/player controls | Partial | Play/pause/seek/speed and sleep-timer arm/cancel exist; route/platform surfaces remain absent or read-only. |
| Queue | Partial | Selection, play, remove, clear, add-last, and play-next are wired; reorder/persistence validation remains. |
| Bookmarks | Partial | Starred episodes now have a tab and unstar/play/queue actions; filtering/search is still absent. |
| Downloads | TUI wired / executor-dependent | Active queue rows, progress, pause/resume/cancel/cancel-all, delete-file routing, and per-episode badges are wired. A completed-download history needs a richer kernel projection. |
| Settings | Partial | Common playback/AI/Nostr toggles, provider/model editors, env-backed terminal credential loading, STT/TTS/local selectors, and configured relay add/remove/role editing are wired; playback interval editors, onboarding, notification detail, and Nostr profile/public relay fields remain. |
| Clips | Partial | Clip projection, AutoSnip, play-from-start, and delete are wired; composer/export/share flows are absent. |
| Agent chat | TUI wired / kernel scaffold | TUI send/clear is wired and tested; real LLM loop remains governed by the existing NMP backlog. |
| Agent tasks/picks/memory/notes | Partial | TUI task CRUD/run, pick play/queue, memory CRUD, note fetch/publish are wired; note trust workflows and the real scheduler/responder loop remain kernel backlog work. |
| Wiki/RAG | Read-only/Scaffold | Articles and search results render; generate/search controls and real RAG synthesis remain. |
| Inbox triage | Partial | Triage rows and in-progress state render; dismiss/retriage controls need wiring. |
| Nostr/social/relays/comments | Read-only/Scaffold | Account, contacts, relays, and comments project; identity, relay editing, comments publish, and social graph completion remain. |
| Voice/transcripts/chapters/ad-skip | Partial | Detail renders transcript/chapter/ad-segment projections and dispatches fetch/compile controls; voice capture, transcript authoring, and platform validation remain. |

## Next Slices

1. Add the remaining settings editors for playback intervals, onboarding,
   notification options, and Nostr profile/public relay fields.
2. Add wiki generation/search plus richer agent note trust/conversation flows
   once the corresponding kernel behavior is real.
3. Add a completed-download history once the kernel projects durable completed
   download rows beyond per-episode `download_path`.
4. Add focused TUI integration scenarios for queue, bookmarks, clips, settings,
   and agent actions, then broaden to network-backed subscribe/search smoke.
