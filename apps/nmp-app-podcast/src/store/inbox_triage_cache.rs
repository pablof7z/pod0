//! JSON persistence for the LLM inbox-triage cache
//! (`HashMap<String, TriageResult>`, keyed by episode_id string).
//!
//! ## Why this exists
//!
//! Inbox triage runs an Ollama LLM call per unlistened episode (see
//! [`crate::inbox_handler::triage_episodes_in_background`]). The scored results
//! live in an in-memory `inbox_triage_cache` shared by the host-op handler and
//! the snapshot reader. Before this module that cache was process-lifetime
//! only, so every cold launch re-triaged the entire unlistened backlog — a slow
//! burst of LLM calls reproducing scores the previous session already computed.
//!
//! This module persists the cache to `<data_dir>/inbox-triage-cache.json` so a
//! cold launch reloads prior scores and only the proactive trigger's normal
//! staleness rules (24h for `Ready`, the retry cooldown for `Pending`) drive
//! any re-triage. The whole map is persisted — `Pending` placeholders included
//! — at face value; a stale `Pending` simply retries naturally once its
//! cooldown has elapsed by the next launch.
//!
//! ## Shape
//!
//! Free functions over a `&Path` (mirroring [`crate::store::relay_config`]),
//! NOT a method on a store struct, because the cache is owned by the FFI
//! handle, not `PodcastStore`. This also keeps the save/load logic out of
//! `inbox_handler.rs`, which sits right under the 500-line hard ceiling.
//!
//! ## D6
//!
//! Both directions degrade silently. A missing file is a fresh start (cold
//! install / first run), not an error. A corrupt or unparseable file loads as
//! an empty map — the next triage pass repopulates it. A write failure leaves
//! the in-memory map authoritative for the session.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::inbox_llm::TriageResult;
use crate::store::PodcastStore;

/// File name written under the bound `data_dir`.
pub const INBOX_TRIAGE_CACHE_FILE: &str = "inbox-triage-cache.json";

/// Write the full triage cache to `<data_dir>/inbox-triage-cache.json`.
///
/// The write is atomic (serialize → `.tmp` → `rename`), matching
/// [`crate::store::podcast_keys`]'s discipline: a torn write must never leave a
/// half-serialized cache that fails to parse on the next launch (which would
/// silently wipe every score). `create_dir_all` first so the very first write
/// to a not-yet-created `data_dir` lands (D6).
///
/// Returns `Err` with a human-readable message on a serialization or write
/// failure so the caller can log it; persistence failure is non-fatal (the
/// in-memory cache stays authoritative for the session).
pub fn save_triage_cache(
    dir: &Path,
    cache: &HashMap<String, TriageResult>,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(cache).map_err(|e| e.to_string())?;
    let final_path = dir.join(INBOX_TRIAGE_CACHE_FILE);
    let tmp_path = dir.join(format!("{INBOX_TRIAGE_CACHE_FILE}.tmp"));
    std::fs::write(&tmp_path, &json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())
}

/// Load the persisted triage cache from `<data_dir>/inbox-triage-cache.json`.
///
/// Returns an empty map when the file is missing (the common fresh-launch
/// case), unparseable, or holds an empty object. A missing file is **not** an
/// error — it just means "no prior triage to restore" (D6).
#[must_use]
pub fn load_triage_cache(dir: &Path) -> HashMap<String, TriageResult> {
    let path = dir.join(INBOX_TRIAGE_CACHE_FILE);
    let Ok(bytes) = std::fs::read(&path) else {
        return HashMap::new();
    };
    serde_json::from_slice::<HashMap<String, TriageResult>>(&bytes).unwrap_or_default()
}

/// Persist the triage cache once a batch finishes, resolving the data dir from
/// the store. Called from `inbox_handler::triage_episodes_in_background` — the
/// single choke point for both the explicit and proactive triage paths.
///
/// Locks are taken briefly and released before file IO: the data dir is read
/// from the store, then the cache map is cloned out, so neither the store nor
/// the cache mutex is held across the write (`build_inbox` locks the cache on
/// every snapshot tick). No data dir bound (unit tests / pre-bind) → no-op. A
/// persistence failure degrades silently; the in-memory cache stays
/// authoritative for the session (D6).
pub fn persist_from_store(
    store: &Arc<Mutex<PodcastStore>>,
    triage_cache: &Arc<Mutex<HashMap<String, TriageResult>>>,
) {
    let Some(dir) = (match store.lock() {
        Ok(s) => s.data_dir().map(Path::to_path_buf),
        Err(_) => None,
    }) else {
        return;
    };
    let snapshot = match triage_cache.lock() {
        Ok(c) => c.clone(),
        Err(_) => return,
    };
    let _ = save_triage_cache(&dir, &snapshot);
}

#[cfg(test)]
#[path = "inbox_triage_cache_tests.rs"]
mod tests;
