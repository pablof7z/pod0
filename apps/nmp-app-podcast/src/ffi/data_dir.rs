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
use std::sync::atomic::Ordering;

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
pub extern "C" fn nmp_app_podcast_set_data_dir(
    handle: *mut PodcastHandle,
    path: *const c_char,
) {
    if handle.is_null() {
        return;
    }
    let Some(path_str) = c_string_opt(path) else { return; };
    if path_str.is_empty() {
        return;
    }
    // SAFETY: caller guarantees `handle` is a valid pointer returned by
    // `nmp_app_podcast_register` and not yet freed.
    let handle = unsafe { &*handle };

    let (loaded, loaded_queue) = match handle.store.lock() {
        Ok(mut s) => {
            let count = s.set_data_dir(PathBuf::from(path_str.clone()));
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
        let had_key = id.secret_hex.is_none();
        id.set_data_dir(&PathBuf::from(&path_str));
        // Return true if we just loaded a key that wasn't there before.
        had_key && id.secret_hex.is_some()
    } else {
        false
    };

    if loaded > 0 || !loaded_queue.is_empty() || identity_loaded {
        // Force the next snapshot poll to pick up the restored library,
        // queue, and/or identity even though no write happened here.
        handle.rev.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[path = "data_dir_tests.rs"]
mod tests;
