//! Disk persistence for [`PodcastStore`].
//!
//! Single JSON file (`podcasts.json`) inside a caller-supplied data directory.
//! Writes are atomic (write to `podcasts.json.tmp` then rename); failures
//! degrade silently per D6 — the in-memory store stays authoritative.
//!
//! ## Wire format
//!
//! ```text
//! {
//!   "schema_version": 1,
//!   "podcasts": [ { "podcast": <Podcast>, "episodes": [<Episode>, ...] }, ... ],
//!   "memory_facts": [ { "id": "...", "key": "...", ... }, ... ]  // optional
//! }
//! ```
//!
//! Versioned so future migrations can detect older payloads. Unknown
//! schema_version is treated as "empty" — the file is replaced on next
//! write. New optional fields (e.g. `memory_facts` added in feature #33)
//! are tagged `#[serde(default)]` so older payloads decode cleanly without
//! bumping the schema and wiping every subscription on upgrade.

use std::path::{Path, PathBuf};

use podcast_core::{Episode, Podcast};
use serde::{Deserialize, Serialize};

use crate::ffi::projections::MemoryFact;
use crate::player::AdSegment;

/// Schema marker for `podcasts.json`. Bump on incompatible format changes.
pub const PERSIST_SCHEMA_VERSION: u32 = 1;

/// File name of the persisted store inside the data directory.
pub const PODCASTS_FILE: &str = "podcasts.json";

/// On-disk envelope. One row per subscribed podcast with its episodes inlined
/// so the load is a single fread.
///
/// `has_completed_onboarding` is part of the same envelope so the iOS
/// shell's `OnboardingView` gate survives restart without a second file.
/// `serde(default)` keeps older saved files (predating the field) loading
/// cleanly as `false`.
/// All fields except `schema_version` and `podcasts` use `#[serde(default)]`
/// so older saved files (pre-dating a field) load without errors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct PersistedStore {
    pub schema_version: u32,
    pub podcasts: Vec<PersistedPodcast>,
    #[serde(default)]
    pub has_completed_onboarding: bool,
    /// Agent memory bag — optional so pre-v2 files decode cleanly.
    #[serde(default)]
    pub memory_facts: Vec<MemoryFact>,
    /// Per-episode ad-break intervals. Sorted on write for deterministic bytes.
    #[serde(default)]
    pub ad_segments: Vec<(String, Vec<AdSegment>)>,
    #[serde(default)]
    pub settings: PersistedSettings,
    /// "Up Next" queue — episode ids in play order. `#[serde(default)]` keeps
    /// pre-existing files (before queue persistence shipped) loading as empty.
    #[serde(default)]
    pub queue: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedSettings {
    /// Mirrors `PodcastStore::auto_skip_ads_enabled`. Defaults to
    /// `false` so an old payload (no settings block) hydrates with
    /// the toggle off — never accidentally enabled.
    #[serde(default)]
    pub auto_skip_ads_enabled: bool,
    /// Skip-forward interval in seconds. `serde(default)` loads pre-existing
    /// files (that lack this field) as 0.0; the store replaces 0.0 with the
    /// semantic default (30.0) during hydration.
    #[serde(default)]
    pub skip_forward_secs: f64,
    /// Skip-backward interval in seconds. Same 0.0 → 15.0 sentinel logic.
    #[serde(default)]
    pub skip_backward_secs: f64,
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            auto_skip_ads_enabled: false,
            skip_forward_secs: 30.0,
            skip_backward_secs: 15.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PersistedPodcast {
    pub podcast: Podcast,
    #[serde(default)]
    pub episodes: Vec<Episode>,
    /// Per-podcast auto-download opt-in flag. `#[serde(default)]` lets the
    /// load path tolerate older `podcasts.json` files written before this
    /// field shipped: missing key ⇒ `false` (auto-download off). We
    /// deliberately do NOT bump `PERSIST_SCHEMA_VERSION` for this addition
    /// — bumping wipes the user's library because `load()` treats unknown
    /// schemas as empty (see this file, line ~60).
    #[serde(default)]
    pub auto_download: bool,
}

/// Resolve the path of `podcasts.json` inside `data_dir`.
pub(super) fn podcasts_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PODCASTS_FILE)
}

/// Load `podcasts.json` from `data_dir`. Returns `Ok(None)` when the file
/// does not exist (fresh install). Any parse / IO error is propagated so the
/// caller can decide whether to log and continue with an empty store.
pub(super) fn load(data_dir: &Path) -> std::io::Result<Option<PersistedStore>> {
    let path = podcasts_path(data_dir);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let store: PersistedStore = serde_json::from_slice(&bytes).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?;
            if store.schema_version != PERSIST_SCHEMA_VERSION {
                // Unknown / future schema — treat as empty; the next mutation
                // will overwrite with the current shape.
                return Ok(None);
            }
            Ok(Some(store))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Atomically write `payload` to `podcasts.json` inside `data_dir`.
///
/// Strategy: serialize → write to `podcasts.json.tmp` → `fs::rename` over the
/// final path. `rename` is atomic on the same filesystem, so the only failure
/// modes are "old file intact" or "new file in place" — never a partial write.
pub(super) fn save(data_dir: &Path, payload: &PersistedStore) -> std::io::Result<()> {
    // Ensure the directory exists. `create_dir_all` is a no-op when present.
    std::fs::create_dir_all(data_dir)?;

    let json = serde_json::to_vec_pretty(payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let final_path = podcasts_path(data_dir);
    let tmp_path = data_dir.join(format!("{PODCASTS_FILE}.tmp"));
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

// Tests split into persistence_tests.rs; #[path] keeps private items in scope.
#[cfg(test)]
#[path = "persistence_tests.rs"]
mod tests;
