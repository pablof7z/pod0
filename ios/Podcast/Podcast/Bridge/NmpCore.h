#ifndef NMP_CORE_H
#define NMP_CORE_H

#include <stdbool.h>
#include <stdint.h>

// Podcast uses the raw C bridge over the NMP kernel actor. This header MUST
// stay in sync with the non-test-gated `#[no_mangle] extern "C" fn nmp_app_*`
// symbols exported from `crates/nmp-ffi/src/`. The M14 UniFFI codegen path
// will supersede this; until then it is hand-maintained and verified by the CI
// gate `ci/check-ffi-header-drift.sh`.

void *nmp_app_new(void);
void nmp_app_free(void *app);

typedef void (*NmpUpdateCallback)(void *context, const char *json);
void nmp_app_set_update_callback(void *app, void *context, NmpUpdateCallback callback);

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
void nmp_app_signin_nsec(void *app, const char *secret);
void nmp_app_signin_bunker(void *app, const char *uri);
void nmp_signer_broker_init(void *app);
void nmp_app_cancel_bunker_handshake(void *app);
char *nmp_app_nostrconnect_uri(void *app, const char *relay_url, const char *callback_scheme);
void nmp_broker_free_string(char *ptr);

// `nmp_app_remove_account` enqueues `ActorCommand::RemoveAccount` for the
// supplied identity id (hex pubkey). The actor drops the row + invalidates
// any cached keys; the next snapshot tick reflects the change.
void nmp_app_remove_account(void *app, const char *identity_id);
// Deliver a JSON-encoded VoiceReport (STT partial/final, listening
// started/stopped, speak started/finished, error) to the Rust voice
// projection. Currently always returns NULL — voice mode has no
// synchronous follow-up command surface yet. Reserved as `char*` so
// future milestones can return a follow-up `VoiceCommand` without an
// ABI break; caller MUST free a non-NULL result via `nmp_app_free_string`.
char *nmp_app_podcast_voice_report(void *handle, const char *report_json);

#endif
