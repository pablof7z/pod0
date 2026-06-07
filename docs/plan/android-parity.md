# Android Feature Parity - Status Matrix

**Goal:** bring the Android Compose shell to feature parity with the iOS app,
built on the same NMP/Rust kernel. Business logic stays in Rust; Android is a
thin rendering + capability shell (mirror of `App/Sources/` on iOS).

**Reference:** `App/Sources/` is the parity specification; the Rust kernel
(`apps/nmp-app-podcast/`) is the shared source of truth. The Android UI lives
in `android/Podcast/app/src/main/java/io/f7z/podcast/`.

## Status Labels

| Label | Meaning |
|---|---|
| Shipped | User-visible behavior works through the NMP stack on Android. |
| Partial | A visible shell exists, but a kernel-routed behavior gap remains. |
| Scaffold | Types / UI / action shells exist, but real behavior is absent. |
| Not started | No Android implementation yet. |

## Tier 1 - Core Usability

| Feature | Status |
|---|---|
| Subscribe via RSS | Shipped |
| Search (iTunes/RSS directory) | Shipped |
| Library / show grid | Shipped |
| Show detail + episode list | Shipped |
| Episode detail view | Shipped |
| Feed refresh (pull-to-refresh) | Shipped |
| Audio playback | Shipped |
| Variable speed | Shipped |
| Sleep timer | Shipped |
| Episode download UI | Shipped |
| Playback settings | Shipped |
| Playback queue | Shipped |
| Lock-screen / media controls | Partial (MediaSession exists; controls still need Rust-routed policy validation) |

## Tier 2 - AI

| Feature | Status |
|---|---|
| Inbox triage | Not started (snapshot field decoded; no inbox UI) |
| Agent chat | Scaffold |
| Transcripts | Not started |
| AI chapters | Not started (chapters render in episode detail; no synthesis trigger) |
| Auto ad-skip | Not started |
| RAG / wiki | Not started |
| Voice mode | Scaffold |
| AI picks / categories | Not started (categories render in episode detail; no picks rail) |

## Tier 3 - Nostr

| Feature | Status |
|---|---|
| Keypair generation | Not started |
| BYOK nsec | Shipped (local nsec import + Android Keystore persistence) |
| NIP-46 bunker | Not started |
| Profile editing | Not started |
| Relay list | Not started |
| NIP-F4 discovery + publish | Not started |
| Episode comments | Not started |
| Social graph | Not started |

## Tier 4 - Platform

| Feature | Status |
|---|---|
| Android Auto | Not started |
| Home-screen widget | Scaffold |
| App Actions | Not started |
| Local notifications | Not started |

## Current Android Parity Baseline

Android is now a real second-platform shell for the Tier 1 podcast flows. It
decodes the Rust snapshot, renders Compose screens, dispatches op-tagged
actions back into the kernel, and executes OS capabilities without owning
podcast business rules.

- **Snapshot + actions.** Library, search, show detail, episode detail,
  downloads, settings, queue, inbox, playback, chapters, categories, and
  identity fields are decoded from the Rust projection. Subscribe, refresh,
  download/delete, play/pause/seek/speed, sleep timer, and settings mutations
  use the same namespace/body dispatch shape as iOS.

- **Capability bridge.** Android registers a generic NMP capability callback
  for HTTP and audio command execution. Feed/search refreshes now run through
  `nmp.http.capability`, and ExoPlayer commands/reports round-trip through
  the Rust player actor. Downloads remain a single-writer pull-model executor
  seeded by `downloads.active` rows so Android does not duplicate the kernel's
  queue policy.

- **Remaining Tier 1 gaps.** MediaSession lock-screen controls exist but still
  need explicit validation that every remote command routes through Rust
  playback policy. Key generation is not exposed yet; Android supports
  imported nsec persistence only.

## Shared Provider / Task Bridge Contract

Android should use the existing generic bridge for all stateful provider
settings and task-intent APIs:

- Task rows arrive via `PodcastSnapshot.agent_tasks`; Android dispatches
  `create`, `delete`, `enable`, `disable`, and `run_now` to the
  `podcast.tasks` namespace with the op-tagged payloads in
  `ui/ActionDispatcher.kt`.
- Provider/model settings arrive via `PodcastSnapshot.settings`; Android
  dispatches provider model and credential metadata changes to the
  `podcast.settings` namespace. API-key values use
  `set_provider_api_keys` as in-memory-only input; Android remains responsible
  for secure storage and reloading those values on app start.
- No provider-specific JNI wrapper is needed for these action/snapshot flows.
  `KernelBridge.dispatchAction(namespace, payloadJson)` already routes through
  the same Rust `ActionModule` registry that iOS uses.

Direct shared-provider transport now has Android JNI parity for the C ABI
symbols already on `main`:

- `nmp_app_podcast_provider_complete(handle, intent_json) -> char*`
- `nmp_app_podcast_provider_embed(handle, intent_json) -> char*`
- `nmp_app_podcast_provider_model_catalog(handle) -> char*`
- `nmp_app_podcast_generate_image(handle, request_json) -> char*`
- `nmp_app_podcast_rerank(handle, request_json) -> char*`

`KernelBridge` exposes handle-scoped `providerComplete`, `providerEmbed`,
`providerModelCatalog`, `generateImage`, and `rerank` methods that return
Rust's JSON envelope after the JNI shim frees the Rust string with
`nmp_app_free_string`. Android model-role settings now load the shared Rust
catalog and dispatch selections through `podcast.settings`. Remaining provider
parity work is secure provider-key reload on Android start and full provider
credential settings screens.
