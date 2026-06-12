//! `nmp_app_podcast_set_data_dir` — bind the podcast library store to a
//! persistence directory and reload any saved state.
//!
//! Swift calls this exactly once, between `nmp_app_podcast_register` and
//! `nmp_app_start`, with the iOS Application Support directory (typically
//! `<app-container>/Library/Application Support/PodcastLibrary/`).
//!
//! After a successful load the function bumps the shared `rev` counter so the
//! next snapshot poll on the main thread surfaces the restored library
//! without waiting for a write to land.

use std::ffi::c_char;
use std::path::PathBuf;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use super::helpers::c_string_opt;

/// Bind `handle`'s store to the directory at `path` and reload `podcasts.json`.
///
/// `path` must be a nul-terminated UTF-8 C string referring to a writable
/// directory (or a path whose parent exists; the function creates the leaf
/// directory if missing). A NULL `path` or `handle`, or a non-UTF-8 path, is
/// a silent no-op (D6).
///
/// Caller contract: invoke once, after `nmp_app_podcast_register`, before
/// `nmp_app_start`. Calling multiple times rebinds the store to the new path
/// and reloads from it.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn nmp_app_podcast_set_data_dir(handle: *mut PodcastHandle, path: *const c_char) {
    if handle.is_null() {
        return;
    }
    let Some(path_str) = c_string_opt(path) else {
        return;
    };
    if path_str.is_empty() {
        return;
    }
    ffi_guard("nmp_app_podcast_set_data_dir", || (), || {
        // SAFETY: caller guarantees `handle` is a valid pointer returned by
        // `nmp_app_podcast_register` and not yet freed.
        let handle = unsafe { &*handle };

        let path_buf = PathBuf::from(path_str.clone());

        let (loaded, loaded_queue) = match handle.store.lock() {
            Ok(mut s) => {
                let count = s.set_data_dir(path_buf.clone());
                let queue = s.take_loaded_queue();
                (count, queue)
            }
            Err(_) => return, // poisoned mutex — degrade silently (D6)
        };

        // Restore the "Up Next" queue from disk. Even an empty persisted queue
        // is fine — the shared PlaybackQueue starts empty and we just skip.
        if !loaded_queue.is_empty() {
            if let Ok(mut q) = handle.queue.lock() {
                for id in &loaded_queue {
                    q.add_to_end(id);
                }
            }
        }

        // Bind the identity store to the same directory. If `identity.json` exists
        // this loads the saved secret key and derives `pubkey_hex` + `npub` so
        // the next snapshot poll surfaces `active_account` without a write.
        let identity_loaded = if let Ok(mut id) = handle.identity.lock() {
            let was_empty = id.secret_hex.is_none();
            id.set_data_dir(&PathBuf::from(&path_str));
            // Only bump rev if we just loaded a key that wasn't present before.
            was_empty && id.secret_hex.is_some()
        } else {
            false
        };

        // Bind the per-podcast NIP-F4 key store to the same directory and reload
        // `podcast-keys.json`. Restored keys mean any owned podcast survives an
        // app restart and re-derives the same `owner_pubkey_hex` in the snapshot.
        // Step 13: podcast_keys now in state.publish (PublishState).
        let keys_loaded = if let Ok(mut keys) = handle.state.publish.podcast_keys.lock() {
            keys.set_data_dir(PathBuf::from(&path_str))
        } else {
            0
        };

        // Restore the user's configured relays from the `.nmp-relay-config.json`
        // sidecar, if present. This is the load half of the C-ABI relay-config
        // persistence (`store::relay_config`); the save half lives in the host-op
        // handler's relay-edit arm.
        //
        // WHY HERE (not in `register`): `register` runs BEFORE any data dir is
        // known, so it cannot read the sidecar. It seeds the declared defaults via
        // `set_initial_relays_for_start` unconditionally. This function runs after
        // `register` and before `nmp_app_start` (see the caller contract above),
        // which is exactly the window `set_initial_relays_for_start` requires — the
        // actor reads the staged `initial_relays` only when it handles
        // `ActorCommand::Start`. So overriding the staged seed here makes the
        // register-time seed genuinely first-install-only: a returning user with a
        // saved sidecar gets their edited list; a fresh install (no sidecar) keeps
        // the defaults `register` already staged.
        //
        // SAFETY: `handle.app` is the live `*mut NmpApp` the handle was registered
        // with; the actor thread is joined before `nmp_app_free`, so the pointer is
        // valid here (this runs before `nmp_app_start`, well before teardown).
        let saved_relays = crate::store::relay_config::load_relay_config(&path_buf);
        if !saved_relays.is_empty() && !handle.app.is_null() {
            unsafe { &*handle.app }.set_initial_relays_for_start(saved_relays);
        }

        // Restore the LLM inbox-triage cache from `inbox-triage-cache.json`, if
        // present. Without this, every cold launch re-triages the whole unlistened
        // backlog (a burst of Ollama calls reproducing scores the prior session
        // already computed). A missing file is a fresh start, not an error (D6).
        // Persistence is written by `inbox_handler::persist_triage_cache` after each
        // triage batch completes.
        let triage_loaded = {
            let restored = crate::store::inbox_triage_cache::load_triage_cache(&path_buf);
            if restored.is_empty() {
                false
            } else if let Ok(mut cache) = handle.state.inbox.triage_cache.lock() {
                // Loaded entries seed the in-memory cache. We only reach this path
                // immediately after `register` (cache constructed empty), so a plain
                // populate is correct — no pre-existing entries to merge against.
                *cache = restored;
                true
            } else {
                false
            }
        };

        // Restore shared agent task rows from `agent-tasks.json`, if present.
        // Missing/corrupt sidecar leaves the register-time seed in place; a valid
        // empty list is loaded so deleting every task remains durable.
        // Step 6: tasks slot is now owned by `state.tasks` (TasksState).
        let tasks_loaded = match crate::store::agent_tasks::load_agent_tasks(&path_buf) {
            Some(restored) => {
                if let Ok(mut tasks) = handle.state.tasks.tasks.lock() {
                    *tasks = restored;
                    true
                } else {
                    false
                }
            }
            None => false,
        };

        if loaded > 0
            || !loaded_queue.is_empty()
            || identity_loaded
            || keys_loaded > 0
            || triage_loaded
            || tasks_loaded
        {
            // Force the next snapshot poll to pick up the restored library,
            // queue, identity, owned-podcast keys, triage cache, and/or tasks
            // even though no write happened here.
            handle.bump_snapshot_rev();
        }
    });
}

#[cfg(test)]
#[path = "data_dir_tests.rs"]
mod tests;
