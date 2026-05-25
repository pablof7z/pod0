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

#endif
