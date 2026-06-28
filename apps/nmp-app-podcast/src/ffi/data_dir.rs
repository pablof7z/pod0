//! `nmp_app_podcast_set_data_dir` — bind the podcast library store to a
//! persistence directory and reload any saved state.
//!
//! Swift calls this exactly once, between `nmp_app_podcast_register` and
//! `nmp_app_start`, with the iOS Application Support directory (typically
//! `<app-container>/Library/Application Support/PodcastLibrary/`).
//!
//! After a successful load the function bumps the shared `rev` counter so the
//! next snapshot delivery on the main thread surfaces the restored library
//! without waiting for a write to land.

use std::ffi::c_char;
use std::path::PathBuf;

use super::guard::ffi_guard;
use super::handle::PodcastHandle;
use super::helpers::c_string_opt;
use crate::nmp_dispatch::activate_local_signer_in_kernel;

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

        // Step 15: store/identity sourced from state.library.
        let (loaded, loaded_queue) = match handle.state.library.store.lock() {
            Ok(mut s) => {
                let count = s.set_data_dir(path_buf.clone());
                let queue = s.take_loaded_queue();
                (count, queue)
            }
            Err(_) => return, // poisoned mutex — degrade silently (D6)
        };

        // Restore the "Up Next" queue from disk. Even an empty persisted queue
        // is fine — the shared PlaybackQueue starts empty and we just skip.
        // Step 14: queue sourced from state.playback.queue.
        let queue_loaded = !loaded_queue.is_empty();
        if queue_loaded {
            if let Ok(mut q) = handle.state.playback.queue.lock() {
                q.restore_items(loaded_queue);
            }
        }

        // Bind the identity store to the same directory. If `identity.json` exists
        // this loads the saved secret key and derives `pubkey_hex` + `npub` so
        // the next snapshot frame surfaces `active_account` without a write.
        let (identity_loaded, loaded_identity_secret) = if let Ok(mut id) = handle.state.library.identity.lock() {
            let was_empty = id.secret_hex.is_none();
            id.set_data_dir(&PathBuf::from(&path_str));
            // Only bump rev if we just loaded a key that wasn't present before.
            let loaded = was_empty && id.secret_hex.is_some();
            (loaded, id.secret_hex.clone())
        } else {
            (false, None)
        };
        if let Some(secret_hex) = loaded_identity_secret.as_deref() {
            activate_local_signer_in_kernel(handle.app, secret_hex);
        }

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

        // Restore Rust-owned clips from `clips.json`, if present. Swift no
        // longer persists a parallel `AppState.clips` mirror; this sidecar is
        // the only durable clip list.
        let clips_loaded = handle.state.clips.set_data_dir(&path_buf);

        // Restore Rust-owned friends from `friends.json`, if present. The
        // projection is the canonical friend list; native shells render it.
        let friends_loaded = handle.state.friends.set_data_dir(&path_buf);

        // Restore the auto-responder dedup + turn-count cache from
        // `agent-note-responder-cache.json`, if present. A missing file is a
        // fresh start (cold install / process restart), not an error (D6). The
        // worst outcome of losing this is a duplicate reply on the first note
        // delivered after a restart, which is acceptable for a v1 responder.
        //
        // NOTE: this cache is intentionally GLOBAL / account-agnostic (one
        // shared file, keyed by globally-unique event/root ids). It is NOT an
        // account-leak: cross-account carryover can only ever suppress a reply,
        // never over-reply (fail-safe). Unlike account-scoped social state, it
        // must persist across identity switches — do not clear it on sign-out.
        let responder_loaded = {
            let restored =
                crate::store::agent_note_responder_cache::load_responder_cache(&path_buf);
            let non_empty =
                !restored.responded_event_ids.is_empty() || !restored.outgoing_turns.is_empty();
            if let Ok(mut cache) = handle.responder_cache.lock() {
                *cache = restored;
                non_empty
            } else {
                false
            }
        };

        // ── Approved-peer store (disk → in-memory approved_peer_store Arc) ──
        //
        // Loaded here so the trust predicate (`(followed || approved) && !blocked`)
        // reflects durable user decisions from the first projection after a restart.
        // Durable: NOT cleared on account switch (per-account data dir means this
        // file is already account-scoped). The in-memory Arc is shared with
        // `state.social` (trust predicate) so no extra seeding step is needed.
        {
            let restored =
                crate::store::approved_peer_store::load_approved_peer_store(&path_buf);
            if let Ok(mut store) = handle.approved_peer_store.lock() {
                *store = restored;
            }
        }

        // ── Outbound-turn cache (disk → in-memory + social projection slot) ─
        //
        // Loaded immediately after the responder cache so the social
        // `nostr_conversations_snapshot()` includes turns from prior sessions
        // on the very first projection after a restart. Like the responder
        // cache this is account-agnostic (keyed by globally-unique event ids);
        // the social slot itself is cleared on account switch.
        let outbound_loaded = {
            let restored =
                crate::store::outbound_turn_cache::load_outbound_turn_cache(&path_buf);
            let non_empty = !restored.is_empty();
            // Seed the in-memory projection slot so the conversation view
            // populates immediately without waiting for a relay re-delivery.
            if non_empty {
                handle.state.social.seed_outbound_turns(restored.turns().to_vec());
            }
            // Keep the disk-persistence Arc in sync.
            if let Ok(mut cache) = handle.outbound_turn_cache.lock() {
                for turn in restored.turns() {
                    cache.record(turn.clone());
                }
            }
            non_empty
        };

        // Bind the knowledge SQLite sidecar and cold-load any persisted chunks.
        // `set_data_dir` opens `knowledge.sqlite`, runs migrations, and seeds the
        // in-memory KnowledgeStore — same data-dir, separate sidecar file (D6).
        let knowledge_loaded = handle.state.knowledge.set_data_dir(&path_buf);

        if loaded > 0
            || queue_loaded
            || identity_loaded
            || keys_loaded > 0
            || triage_loaded
            || tasks_loaded
            || clips_loaded
            || friends_loaded
            || responder_loaded
            || outbound_loaded
            || knowledge_loaded > 0
        {
            // Force the next snapshot frame to pick up the restored library,
            // queue, identity, owned-podcast keys, triage cache, and/or tasks
            // even though no write happened here.
            handle.bump_snapshot_rev();
        }
    });
}

#[cfg(test)]
#[path = "data_dir_tests.rs"]
mod tests;
