# Shared LLM And Task Architecture

This plan captures the architecture contract for provider transport, model
selection, and agent task creation across iOS, Android, and the TUI.

## Contract

Platform shells own:

- Credential capture and secret storage in their platform-native secure store.
- Native browser presentation and callback capture for BYOK authorization.
- Delivery of live API keys to Rust through the existing in-memory settings
  action (`podcast.settings`, `set_provider_api_keys`) for OpenRouter, Ollama,
  ElevenLabs, AssemblyAI, and Perplexity.
- Model-selection UI and display labels.
- Rendering, keyboard/touch interaction, local files, audio, downloads, and
  other platform capabilities.

The shared Rust backend owns:

- Provider routing for OpenRouter, Ollama, Perplexity/OpenRouter-backed online
  search, and local models.
- HTTP request and response shape for provider chat/completions.
- HTTP request and response shape for provider embeddings.
- HTTP request, multipart upload, and response shape for provider speech-to-text
  when the provider is network-owned rather than platform-native.
- Provider credential requirements and error messages.
- BYOK provider scope mapping, PKCE state/verifier generation, callback
  validation, and token exchange request/response parsing.
- Role-to-model resolution and validation.
- Agent task intent resolution and dispatch payload construction.

Platform code must not construct OpenRouter or Ollama chat/embedding request
bodies, Perplexity/OpenRouter online-search requests, OpenRouter, ElevenLabs,
or AssemblyAI speech-to-text requests, ElevenLabs validation requests, provider
URLs, provider-specific auth headers, or raw backend task namespace/body JSON
for normal user workflows.
Platform code may present provider authorization UI and persist returned
secrets, but must not construct BYOK provider scopes, PKCE challenges, callback
state validation, or `/api/token` request/response logic.

## Provider Transport

Current shared Rust provider entry points cover agent chat through
`nmp_app_podcast_chat_complete`, provider-blind single-turn completions through
`nmp_app_podcast_provider_complete`, embeddings through
`nmp_app_podcast_provider_embed`, OpenRouter and ElevenLabs key validation,
provider model catalog discovery, image generation, reranking, OpenRouter
Whisper/STT, ElevenLabs Scribe/STT, AssemblyAI STT, and Perplexity/OpenRouter
online search through `nmp_app_podcast_perplexity_search`.
BYOK provider authorization uses `nmp_app_podcast_byok_authorization` and
`nmp_app_podcast_byok_exchange`; platforms supply app/browser facts and the
callback URL while Rust owns provider scopes, PKCE, state validation, and token
exchange parsing.
Swift live wiki/title/categorization/chapter/clip completion callers now route
through `ProviderCompletionClient` without preflighting Keychain keys, so missing
provider credentials are reported by Rust. Swift OpenRouter settings validation
also calls the shared validator directly, leaving missing-key handling to Rust.
Swift Episode Diagnostics now exposes forced OpenRouter Whisper retry without a
Keychain preflight so the shared Rust STT transport reports setup/provider
errors. Swift ElevenLabs settings validation now calls the shared Rust
validator, leaving `/v1/user` URL/header/response parsing in Rust. ElevenLabs
Scribe now uses `nmp_app_podcast_elevenlabs_scribe_transcribe`; platform
callers submit a typed audio-source intent and Rust owns selected Scribe model
lookup, ElevenLabs auth, local-file/source_url multipart shaping, status
handling, and response parsing. AssemblyAI now uses
`nmp_app_podcast_assemblyai_transcribe`; platform callers submit a typed
audio-source intent and Rust owns selected model fallback lookup, AssemblyAI
auth, `/v2/transcript` submit/poll, status handling, response parsing, and
usage telemetry normalization.
Agent online search now uses `nmp_app_podcast_perplexity_search`; platform
callers submit a typed query intent and Rust owns the direct Perplexity
`/v1/sonar` request, OpenRouter fallback request, credential priority, status
handling, and citation/search-result parsing.
Android mirrors the shared STT/ElevenLabs settings projection, stores
ElevenLabs/AssemblyAI/Perplexity keys in encrypted host storage, reports STT
key presence to Rust, reloads ElevenLabs, AssemblyAI, and Perplexity into the
shared provider-key cache, calls shared Rust ElevenLabs validation plus
Scribe/AssemblyAI transcription and online search through generated UniFFI,
exposes the shared agent chat completion path through generated UniFFI, and updates STT/TTS/voice
selections through typed settings actions. The provider catalog now exposes
both provider-native IDs and `selection_model_id`; iOS, Android, and TUI model
selectors store the selection ID so OpenRouter/Ollama routing survives the
settings round trip. The TUI loads OpenRouter/Ollama/ElevenLabs/AssemblyAI/
Perplexity env credentials into the same shared key-cache action.
Shared routing treats blank and `"none"` credential sources as disconnected so
platform clear actions cannot route bare model IDs through OpenRouter.

Immediate targets:

- Swift clients should keep stubbed test modes, but every live provider
  inference call should route through Rust.
- Android should expose every shared Rust provider function through generated
  UniFFI bridge calls when a user-facing Android feature needs it, and keep
  provider/model settings as typed `podcast.settings` actions instead of
  platform-local state.
- OpenRouter Whisper/STT uses `nmp_app_podcast_openrouter_whisper_transcribe`;
  platform callers submit a typed audio-source intent and Rust owns the
  selected model lookup, OpenRouter auth, remote-source staging, multipart
  upload, status handling, and response parsing.
- Streaming voice-mode STT/TTS remains blocked on the canonical NMP capability
  seam tracked upstream in
  `pablof7z/nostr-multi-platform#954`; do not invent an app-local streaming
  provider protocol as a workaround.

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
- Agent task snapshots project `intent_type`, `intent_label`, and
  `intent_detail` for UI rendering while keeping raw dispatch
  namespace/body fields Rust-internal.
- The TUI task editor accepts typed/natural input such as
  `daily | triage inbox` or `weekly | remember topic=rust` and submits
  `AgentTaskIntent` through the shared backend action.
- Android task creation now uses `create_from_intent` with a variant-backed
  `AgentTaskIntent` payload instead of raw action namespace/body JSON.
- The parked `ios/Podcast` shell mirrors typed task creation so it does not
  reintroduce raw task dispatch JSON if that tree is revived.
- Swift and Android snapshot mirrors no longer require raw dispatch fields for
  agent task rows.
- Swift's scheduled prompt surface now creates/updates `agent_prompt` tasks
  through `podcast.tasks`, renders `PodcastUpdate.agentTasks`, and dispatches
  `run_due` on app foreground so due policy lives in Rust. TUI natural task
  input also accepts explicit prompt intents (`prompt: ...`).

Remaining targets:

- Keep raw dispatch namespace/body JSON out of all normal user-facing task
  creation workflows.
- Persist `agent_tasks` across kernel restarts and add a durable
  background-agent execution/history model if prompt tasks should stay isolated
  from the main agent chat transcript.

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
3. Android bridge parity PR: expose any new shared provider/task APIs through
   generated UniFFI bridge calls if they are not already reachable.
4. Push-update PR: replace TUI-specific revision polling with shared backend
   update delivery for autonomous state changes.
5. Validation PR: run real TUI/tmux scenarios with `glm-5.1:cloud`, plus focused
   iOS/Android checks for provider settings and model selection.
