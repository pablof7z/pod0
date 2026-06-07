#ifndef NMP_CORE_H
#define NMP_CORE_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

// Podcast uses the raw C bridge over the NMP kernel actor. This header MUST
// stay in sync with the non-test-gated `#[no_mangle] extern "C" fn nmp_app_*`
// symbols exported from `crates/nmp-ffi/src/`. The M14 UniFFI codegen path
// will supersede this; until then it is hand-maintained and verified by the CI
// gate `ci/check-ffi-header-drift.sh`.

void *nmp_app_new(void);
void nmp_app_free(void *app);

// The kernel's update transport is binary FlatBuffers: the callback receives a
// length-delimited byte buffer, NOT a NUL-terminated JSON string. Decode it to
// the JSON envelope via `nmp_app_podcast_decode_update_frame`.
typedef void (*NmpUpdateCallback)(void *context, const uint8_t *bytes, size_t len);
void nmp_app_set_update_callback(void *app, void *context, NmpUpdateCallback callback);

// Decode a binary FlatBuffers update frame `(bytes, len)` into the JSON envelope
// the shell consumes: `{"t":"snapshot","v":...}` or `{"t":"panic","message":...}`.
// Returns a heap string to free with `nmp_app_free_string`, or NULL on a frame
// that isn't decodable.
char *nmp_app_podcast_decode_update_frame(const uint8_t *bytes, size_t len);

// Persistent storage directory for the LMDB EventStore backend. Must be
// called before `nmp_app_start`; a NULL or empty `path` clears it.
void nmp_app_set_storage_path(void *app, const char *path);

void nmp_app_start(void *app, unsigned int events_per_second, unsigned int visible_limit, unsigned int emit_hz);
void nmp_app_configure(void *app, unsigned int events_per_second, unsigned int visible_limit, unsigned int emit_hz);
void nmp_app_stop(void *app);
void nmp_app_reset(void *app);

// Actor-liveness probe (D7 pull-side). Returns 1 when running, 0 when
// terminated or not started. A null app is 0.
uint8_t nmp_app_is_alive(void *app);

// T118 / G3 — iOS scenePhase → kernel lifecycle bridge. Fire-and-forget (D6).
void nmp_app_lifecycle_foreground(void *app);
void nmp_app_lifecycle_background(void *app);

void nmp_app_free_string(char *ptr);

// ── T151 — generic dispatch ───────────────────────────────────────────────
//
// `nmp_app_dispatch_action` is the single namespace-keyed entry point for the
// ActionModule family. Returns a heap-allocated JSON string
// `{"correlation_id":"<hex>"}` on accept or `{"error":"..."}` on rejection;
// the caller MUST release via `nmp_app_free_string`. D6: never NULL for a
// non-NULL app.
char *nmp_app_dispatch_action(void *app, const char *namespace, const char *action_json);

// ── Capability callback ───────────────────────────────────────────────────
//
// `nmp_app_set_capability_callback` registers a native handler for all
// kernel-issued `CapabilityRequest` JSON envelopes (HTTP, keyring, audio …).
// The callback receives a JSON request string (caller-owned) and MUST return
// a freshly malloc-allocated JSON `CapabilityEnvelope` string; Rust takes
// ownership via `CString::from_raw`, so the Swift implementation MUST use
// `strdup` (not a Swift `String` pointer). Passing `NULL` for `callback`
// unregisters; unregistered requests come back as error envelopes (D6).

typedef char *(*NmpCapabilityCallback)(void *context, const char *request_json);
void nmp_app_set_capability_callback(void *app, void *context, NmpCapabilityCallback callback);

// ── nmp-app-podcast per-app FFI ──────────────────────────────────────────
//
// `libnmp_app_podcast.a` is the Podcast Rust aggregate archive (D0: protocol
// glue outside nmp-core).
//
// Flow:
// 1. Call `nmp_app_podcast_register(app)` once after `nmp_app_new()`. Returns
//    an opaque handle (or NULL on failure).
// 2. On each render tick call `nmp_app_podcast_snapshot(handle)` to get a
//    nul-terminated JSON string. The caller owns the pointer until it calls
//    `nmp_app_podcast_snapshot_free(ptr)`.
// 3. On teardown call `nmp_app_podcast_unregister(handle)` BEFORE
//    `nmp_app_free(app)`.
//
// Fire-and-forget: every entry point degrades silently on null pointers,
// poisoned mutexes, or serialization failure (D6).
void *nmp_app_podcast_register(void *app);
char *nmp_app_podcast_snapshot(void *handle);
uint64_t nmp_app_podcast_snapshot_rev(void *handle);
void nmp_app_podcast_snapshot_free(char *ptr);
void nmp_app_podcast_unregister(void *handle);

// Bind the podcast library store to a persistence directory. Must be called
// once between `nmp_app_podcast_register` and `nmp_app_start`. `path` must be
// a nul-terminated UTF-8 C string pointing at a writable directory (created
// if missing). Passing a NULL handle, NULL path, or empty path is a silent
// no-op (D6). Subsequent mutations (subscribe / unsubscribe / refresh) flush
// to `<path>/podcasts.json` atomically.
void nmp_app_podcast_set_data_dir(void *handle, const char *path);

// Deliver a JSON-encoded AudioReport to the Rust PlayerActor.
// Returns a malloc-allocated JSON AudioCommand the caller should execute, or
// NULL when no follow-up is needed. Caller MUST free via `nmp_app_free_string`.
char *nmp_app_podcast_audio_report(void *handle, const char *report_json);

// Deliver a JSON-encoded DownloadReport to the Rust PodcastStore.
// Returns a malloc-allocated JSON DownloadCommand the caller should execute,
// or NULL when no follow-up is needed (today: always NULL — see
// `apps/nmp-app-podcast/src/ffi/download_report.rs`). Caller MUST free via
// `nmp_app_free_string`.
char *nmp_app_podcast_download_report(void *handle, const char *report_json);
// ── Identity / NIP-46 remote-signer FFI ───────────────────────────────────
//
// `nmp_app_signin_nsec` / `nmp_app_signin_bunker` enqueue the matching
// `ActorCommand` on the NMP-core actor (declared in
// `crates/nmp-ffi/src/identity.rs`). `secret` for `nmp_app_signin_nsec` is the
// user's bech32 `nsec1…` (or hex) string; the actor wraps it in `Zeroizing`
// immediately. Hosts MUST NOT log the secret value at any point.
//
// `nmp_app_signin_bunker` accepts a `bunker://` URI and is a silent no-op
// (D6) unless `nmp_signer_broker_init` has been called first.
//
// `nmp_signer_broker_init` registers the bunker hook + relay listener with
// `nmp-core`. Idempotent — second and later calls do nothing. MUST be called
// once after `nmp_app_new()` and before any `bunker://` / `nostrconnect://`
// sign-in attempt.
//
// `nmp_app_cancel_bunker_handshake` aborts the in-flight handshake (if any).
// Idempotent / safe when no handshake is in flight.
//
// `nmp_app_nostrconnect_uri` returns a freshly minted client-initiated
// `nostrconnect://` URI string. The caller MUST free the returned pointer via
// `nmp_broker_free_string`. `relay_url` may be NULL — Rust selects the first
// write-capable relay from the kernel relay-edit projection in that case.
// `callback_scheme` may be NULL — when non-null Rust appends a percent-encoded
// `&callback=<scheme>` query parameter so the signer app can deep-link back.
// Pass NULL when the host scheme is not registered with the OS.
void nmp_app_signin_nsec(void *app, const char *secret, uint8_t make_active);
void nmp_app_signin_bunker(void *app, const char *uri, uint8_t make_active);

// `nmp_app_create_new_account` generates a keypair and publishes kind:0 + the
// relay list. `make_active = 1` activates the new account immediately
// (standard onboarding); `make_active = 0` registers it without switching the
// active session (agent/secondary accounts). `profile_json` is a flat
// string-map; `relays_json` is `[[url, role], …]`.
void nmp_app_create_new_account(void *app,
                                const char *profile_json,
                                const char *relays_json,
                                bool mls,
                                uint8_t make_active);

// D13 sign-and-return — sign a draft event with the named (or active) account
// WITHOUT publishing it. `account_pubkey_hex` is the hex pubkey of the signer
// to use; pass the empty string ("") to sign with the active account.
// `unsigned_json` is `{"kind":N,"content":"…","tags":[…],"created_at":N}` —
// `created_at` is advisory (the kernel re-stamps it, D7). Returns a heap
// `correlation_id` C string the caller MUST free via `nmp_app_free_string`;
// the signed flat-NIP-01 JSON is delivered ASYNC in the `signed_events`
// snapshot projection keyed by that id (`{ "ok": true, "signed_json": "…" }`
// or `{ "ok": false, "error": "…" }`). The host MUST register its
// continuation BEFORE calling so it does not miss the single drain-on-emit
// frame that carries the result.
char *nmp_app_sign_event_for_return(void *app,
                                    const char *account_pubkey_hex,
                                    const char *unsigned_json);
void nmp_signer_broker_init(void *app);
void nmp_app_cancel_bunker_handshake(void *app);
char *nmp_app_nostrconnect_uri(void *app, const char *relay_url, const char *callback_scheme);
void nmp_broker_free_string(char *ptr);

// `nmp_app_remove_account` enqueues `ActorCommand::RemoveAccount` for the
// supplied identity id (hex pubkey). The actor drops the row + invalidates
// any cached keys; the next snapshot tick reflects the change.
void nmp_app_remove_account(void *app, const char *identity_id);

// ── Profile claim / release (T114 reference-first profile resolution) ─────
//
// `nmp_app_claim_profile` registers a refcounted interest in `pubkey`'s kind:0
// profile keyed by `consumer_id`. On the cold-claim transition the kernel
// enqueues a kind:0 REQ against its configured relay pool (or queues it until a
// relay connects), owning all relay/cache policy. The resolved profile surfaces
// in `projections.resolved_profiles` (and `claimed_profiles`) on the next
// snapshot tick — i.e. it rides the same reactive push the shell already folds
// into `nostrProfileCache` via `mergeResolvedProfiles`. This is the designed
// replacement for a host opening its own websocket to fetch kind:0.
//
// `nmp_app_release_profile` decrements the per-consumer refcount; the kernel
// drops the pending request when the last consumer releases. Both are
// FFI-clean (D6): a null/invalid pubkey or consumer id is a silent no-op.
// `pubkey` MUST be lowercase hex; `consumer_id` is a host-chosen stable token
// (typically the view identity) so claims dedupe and release matches claim.
// Declared per `crates/nmp-ffi/src/timeline.rs`.
void nmp_app_claim_profile(void *app, const char *pubkey, const char *consumer_id);
void nmp_app_release_profile(void *app, const char *pubkey, const char *consumer_id);
// Deliver a JSON-encoded VoiceReport (STT partial/final, listening
// started/stopped, speak started/finished, error) to the Rust voice
// projection. Currently always returns NULL — voice mode has no
// synchronous follow-up command surface yet. Reserved as `char*` so
// future milestones can return a follow-up `VoiceCommand` without an
// ABI break; caller MUST free a non-NULL result via `nmp_app_free_string`.
char *nmp_app_podcast_voice_report(void *handle, const char *report_json);

// Deliver a JSON-encoded NetworkReport (nmp.network.capability ConnectivityChanged)
// to the kernel. Updates the Wi-Fi state flag used by the auto-download policy.
// Always returns NULL — no follow-up command.
char *nmp_app_podcast_network_report(void *handle, const char *report_json);

// Deliver a completed transcript (plain text) to the Rust store so AI features
// can access it. JSON shape: {"episode_id":"<uuid>","text":"<plain text>"}.
// Always returns NULL.
char *nmp_app_podcast_transcript_report(void *handle, const char *report_json);

// Fetch the kernel's per-episode pipeline event log (download / transcript /
// identify lifecycle) for one episode, lazily — these events deliberately do
// NOT ride the library snapshot. `episode_id` is a hyphenated UUID string.
// Returns a heap JSON array of event objects (possibly empty `[]`) decoded on
// the Swift side into `[EpisodeAuditEvent]`; the caller MUST free a non-NULL
// result via `nmp_app_free_string`. NULL only on a hard error (D6).
char *nmp_app_podcast_episode_events(void *handle, const char *episode_id);

// ── Provider-blind single-turn LLM completion ─────────────────────────────
//
// `nmp_app_podcast_chat_complete` drives one LLM turn through the Rust
// backend, hiding all provider/credential details from Swift. Swift passes the
// full OpenAI-format message array as a JSON string and receives the assistant's
// text back.
//
// `messages_json` — JSON array of {"role":"…","content":"…"} objects. The
// system prompt must be the first entry (role = "system"). Tool-call turns
// are supported (role = "tool", role = "assistant" with tool_calls).
//
// Returns a heap-allocated JSON string of the form:
//   {"text":"<assistant reply>"}   on success
//   {"error":"<reason>"}           on failure (model unreachable, bad input, …)
// The caller MUST free the returned pointer via `nmp_app_free_string`.
// D6: never returns NULL for a non-null handle.
//
// Threading: this call BLOCKS the calling thread while the network round-trip
// completes. Swift MUST call it from a background thread / detached Task.
char *nmp_app_podcast_chat_complete(void *handle, const char *messages_json);

// Generic provider transport for non-agent one-turn completions. Swift passes a
// provider/model/prompt intent JSON; Rust owns provider URLs, headers, request
// bodies, credential lookup, and response decoding. Response:
//   {"result":{"text":"...","provider":"...","model":"...","latency_ms":0,
//              "usage":{...}?,"prompt_tokens":0,"completion_tokens":0}}
// or {"error":"..."}.
char *nmp_app_podcast_provider_complete(void *handle, const char *intent_json);

// Generic provider transport for embeddings. Swift passes provider/model/input
// intent JSON; Rust owns OpenRouter/Ollama embedding request shaping.
// Response:
//   {"result":{"embeddings":[[...]],"provider":"...","model":"...",
//              "latency_ms":0,"usage":{...}?,"prompt_tokens":0}}
// or {"error":"..."}.
char *nmp_app_podcast_provider_embed(void *handle, const char *intent_json);

// Shared provider model catalog. Rust owns OpenRouter/models.dev/Ollama model
// discovery HTTP, credentials, URL derivation, response DTO parsing, and
// normalized compatibility metadata. Response:
//   {"result":{"models":[...]}}
// or {"error":"..."}.
// The caller MUST free the returned pointer via `nmp_app_free_string`.
// Threading: this call BLOCKS; call from a background thread / detached Task.
char *nmp_app_podcast_provider_model_catalog(void *handle);

// Shared OpenRouter `/auth/key` validation using mirrored provider credentials.
// Returns {"result":...} or {"error":{"kind":"...","message":"..."}}.
// The caller MUST free the returned pointer via `nmp_app_free_string`.
// Threading: this call BLOCKS; call from a background thread / detached Task.
char *nmp_app_podcast_validate_openrouter_key(void *handle);

// Shared ElevenLabs `/v1/user` validation using mirrored provider credentials.
// Returns {"result":...} or {"error":{"kind":"...","message":"..."}}.
// The caller MUST free the returned pointer via `nmp_app_free_string`.
// Threading: this call BLOCKS; call from a background thread / detached Task.
char *nmp_app_podcast_validate_elevenlabs_key(void *handle);

// Shared OpenRouter Whisper speech-to-text transport. Swift passes a typed
// intent JSON:
//   {"audio_url":"file:///.../episode.mp3","language_hint":"en"?}
// Rust owns OpenRouter auth, model selection, upload/download shaping, and
// response parsing. Returns {"result":...} or
// {"error":{"kind":"...","message":"...","status_code":...}}.
// The caller MUST free the returned pointer via `nmp_app_free_string`.
// Threading: this call BLOCKS; call from a background thread / detached Task.
char *nmp_app_podcast_openrouter_whisper_transcribe(void *handle, const char *intent_json);

// Shared ElevenLabs Scribe speech-to-text transport. Swift passes a typed
// intent JSON:
//   {"audio_url":"file:///.../episode.mp3","language_hint":"en"?}
// Rust owns ElevenLabs auth, model selection, local-file/source_url shaping,
// provider status handling, and response parsing. Returns {"result":...} or
// {"error":{"kind":"...","message":"...","status_code":...}}.
// The caller MUST free the returned pointer via `nmp_app_free_string`.
// Threading: this call BLOCKS; call from a background thread / detached Task.
char *nmp_app_podcast_elevenlabs_scribe_transcribe(void *handle, const char *intent_json);

// ── Provider-blind image generation ─────────────────────────────────────
//
// Drives OpenRouter image generation through shared Rust provider transport.
// Swift passes provider intent as JSON:
//   {"prompt":"…","model":"…"}
// and receives:
//   {"image_base64":"<bytes>"} on success
//   {"error":"<reason>"}      on failure
// The caller MUST free the returned pointer via `nmp_app_free_string`.
// Threading: this call BLOCKS; call from a background thread / detached Task.
char *nmp_app_podcast_generate_image(void *handle, const char *request_json);

// ── RAG reranking ───────────────────────────────────────────────────────
//
// Provider-owned reranking transport. Swift sends a provider-neutral JSON
// request:
//   {"model":"cohere/rerank-v3.5","query":"...","documents":["..."],"top_n":10}
// Rust owns the OpenRouter endpoint, auth headers, HTTP body DTO, status
// handling, and response parsing.
//
// Returns a heap-allocated JSON string:
//   {"indices":[0,2,1]}                                    on success
//   {"error":{"kind":"missing_api_key","message":"..."}}   on failure
// The caller MUST free the returned pointer via `nmp_app_free_string`.
// Threading: this call BLOCKS while the network round-trip completes. Swift
// MUST call it from a background thread / detached Task.
char *nmp_app_podcast_rerank(void *handle, const char *request_json);

// ── Local LLM registration ──────────────────────────────────────────────
//
// Register a local LLM backend callback. The callback receives a context pointer
// and a JSON prompt string, and returns a malloc-allocated JSON response string.
// Rust owns the response string lifetime and frees it via nmp_app_free_string.
typedef char* (*NmpLocalLlmFn)(void* context, const char* prompt_json);
void nmp_app_register_local_llm(void* handle, void* context, NmpLocalLlmFn fn);
void nmp_app_clear_local_llm(void* handle);

#endif
