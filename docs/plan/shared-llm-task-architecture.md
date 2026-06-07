# Shared LLM And Task Architecture

This plan captures the architecture contract for provider transport, model
selection, and agent task creation across iOS, Android, and the TUI.

## Contract

Platform shells own:

- Credential capture and secret storage in their platform-native secure store.
- Delivery of live API keys to Rust through the existing in-memory settings
  action (`podcast.settings`, `set_provider_api_keys`) for OpenRouter, Ollama,
  and ElevenLabs.
- Model-selection UI and display labels.
- Rendering, keyboard/touch interaction, local files, audio, downloads, and
  other platform capabilities.

The shared Rust backend owns:

- Provider routing for OpenRouter, Ollama, and local models.
- HTTP request and response shape for provider chat/completions.
- HTTP request and response shape for provider embeddings.
- HTTP request, multipart upload, and response shape for provider speech-to-text
  when the provider is network-owned rather than platform-native.
- Provider credential requirements and error messages.
- Role-to-model resolution and validation.
- Agent task intent resolution and dispatch payload construction.

Platform code must not construct OpenRouter or Ollama chat/embedding request
bodies, OpenRouter or ElevenLabs speech-to-text multipart requests,
ElevenLabs validation requests, provider URLs, provider-specific auth headers,
or raw backend task namespace/body JSON for normal user workflows.

## Provider Transport

Current shared Rust provider entry points cover agent chat through
`nmp_app_podcast_chat_complete`, provider-blind single-turn completions through
`nmp_app_podcast_provider_complete`, embeddings through
`nmp_app_podcast_provider_embed`, OpenRouter and ElevenLabs key validation,
provider model catalog discovery, image generation, reranking, OpenRouter
Whisper/STT, and ElevenLabs Scribe/STT.
Swift live wiki/title/categorization/chapter/clip completion callers now route
through `WikiOpenRouterClient` without preflighting Keychain keys, so missing
provider credentials are reported by Rust. Swift OpenRouter settings validation
also calls the shared validator directly, leaving missing-key handling to Rust.
Swift Episode Diagnostics now exposes forced OpenRouter Whisper retry without a
Keychain preflight so the shared Rust STT transport reports setup/provider
errors. Swift ElevenLabs settings validation now calls the shared Rust
validator, leaving `/v1/user` URL/header/response parsing in Rust. ElevenLabs
Scribe now uses `nmp_app_podcast_elevenlabs_scribe_transcribe`; platform
callers submit a typed audio-source intent and Rust owns selected Scribe model
lookup, ElevenLabs auth, local-file/source_url multipart shaping, status
handling, and response parsing. AssemblyAI STT retries remain Swift-key-gated
until that transport is shared.
Android mirrors the shared STT/ElevenLabs settings projection, stores
ElevenLabs/AssemblyAI keys in encrypted host storage, reports STT key presence
to Rust, reloads ElevenLabs into the shared provider-key cache, calls shared
Rust ElevenLabs validation and Scribe transcription through JNI, and updates
STT/TTS/voice selections through typed settings actions.

Immediate targets:

- Swift clients should keep stubbed test modes, but every live provider
  inference call should route through Rust.
- Android should expose every shared Rust provider function through JNI when a
  user-facing Android feature needs it, and keep provider/model settings as
  typed `podcast.settings` actions instead of platform-local state.
- OpenRouter Whisper/STT uses `nmp_app_podcast_openrouter_whisper_transcribe`;
  platform callers submit a typed audio-source intent and Rust owns the
  selected model lookup, OpenRouter auth, remote-source staging, multipart
  upload, status handling, and response parsing.
- AssemblyAI STT should move to the same shared transport pattern; Rust should
  own the `/v2/transcript` submit/poll contract, selected model lookup, auth
  header, provider status handling, response parsing, and usage telemetry
  normalization.

Provider model-list discovery can remain UI-owned temporarily if it is only a
catalog/browser concern, but any provider inference call must use Rust.

## Task Intents

Task creation must submit typed user intent, not raw backend dispatch payloads.
The backend may continue storing an internal dispatch namespace/body for
`run_now` compatibility, but that payload is not a UI contract.

Current state:

- Rust owns `AgentTaskIntent`, typed task creation, and intent-to-dispatch
  resolution inside `tasks_handler.rs`.
- Raw `create` remains for compatibility/internal callers only.
- The TUI task editor accepts typed/natural input such as
  `daily | triage inbox` or `weekly | remember topic=rust` and submits
  `AgentTaskIntent` through the shared backend action.

Remaining targets:

- Audit Swift and Android task-creation surfaces and migrate any raw task
  creation to `AgentTaskIntent`.
- Keep raw dispatch namespace/body JSON out of all normal user-facing task
  creation workflows.

## Push Updates

The TUI currently depends on periodic snapshot revision checks for autonomous
backend changes. The long-term shared fix is a backend update signal that fires
when app-owned async work mutates shared projection state and bumps `rev`.

Immediate targets:

- Audit which async host-side paths bump `rev` without triggering the NMP
  update sink.
- Add or expose a shared notification seam if the existing projection registry
  cannot emit after those mutations.
- Keep TUI polling only as a temporary fallback tracked in `docs/BACKLOG.md`.

## PR Sequencing

1. Provider transport PR: keep migrating any remaining live provider inference
   paths to shared Rust APIs and delete platform-side credential preflights.
2. Typed task intent follow-up: migrate any remaining Swift/Android task
   creation surfaces to the shared `AgentTaskIntent` contract.
3. Android/JNI parity PR: expose any new shared provider/task APIs through the
   Android bridge if they are not already reachable.
4. Push-update PR: replace TUI-specific revision polling with shared backend
   update delivery for autonomous state changes.
5. Validation PR: run real TUI/tmux scenarios with `glm-5.1:cloud`, plus focused
   iOS/Android checks for provider settings and model selection.
