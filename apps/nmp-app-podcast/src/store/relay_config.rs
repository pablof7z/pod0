//! JSON sidecar persistence for the podcast app's configured relay list,
//! on the raw C-ABI start path.
//!
//! ## Why this exists
//!
//! Relay state (`AppRelaySlot`) is kernel-owned, in-memory state. The
//! `nmp-app-template` builder persists it via its own
//! `relay_config::{load,save}` inside `NmpAppBuilder::start`, but the podcast
//! app is constructed over the raw C-ABI (`nmp_app_new` →
//! `nmp_app_podcast_register` → `nmp_app_start`) and never runs through the
//! builder. So before this module, user relay edits via
//! `podcast.settings.{add_relay,remove_relay,set_relay_role}` mutated the
//! in-memory slot but were lost on restart.
//!
//! This module is the C-ABI-path equivalent of the template sidecar. It writes
//! the SAME on-disk file (`{data_dir}/.nmp-relay-config.json`) with the SAME
//! `[{"url": ..., "role": ...}, ...]` shape, so the two start paths share one
//! canonical representation — a builder-launched and a C-ABI-launched process
//! pointed at the same data dir read and write the identical sidecar.
//!
//! - `save_relay_config` is called by the host-op handler after every relay
//!   mutation, with the full post-mutation relay list read back from the
//!   kernel slot (the source of truth).
//! - `load_relay_config` is called from `nmp_app_podcast_set_data_dir`; a
//!   non-empty result overrides the register-time default seed so a returning
//!   user gets their edited list and the seed becomes first-install-only.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Sidecar filename, shared verbatim with `nmp-app-template`'s relay-config
/// sidecar so both start paths read/write one canonical file.
pub(crate) const RELAY_CONFIG_FILENAME: &str = ".nmp-relay-config.json";

/// One persisted `(url, role)` row. `role` is the kernel's normalized,
/// possibly comma-joined NIP-65 role string (e.g. `"both,indexer"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RelayEntry {
    url: String,
    role: String,
}

/// Write the full relay list to `{data_dir}/.nmp-relay-config.json`.
///
/// Returns `Err` with a human-readable message on a serialization or write
/// failure so the caller can log it; the caller treats persistence failure as
/// non-fatal (the in-memory edit still took effect for the session).
pub fn save_relay_config(data_dir: &Path, relays: &[(String, String)]) -> Result<(), String> {
    let entries: Vec<RelayEntry> = relays
        .iter()
        .map(|(url, role)| RelayEntry {
            url: url.clone(),
            role: role.clone(),
        })
        .collect();
    let json = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
    let path = data_dir.join(RELAY_CONFIG_FILENAME);
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Load the persisted relay list from `{data_dir}/.nmp-relay-config.json`.
///
/// Returns an empty `Vec` when the sidecar is missing, unparseable, or holds
/// an empty array. An empty result means "no persisted config" — the caller
/// falls back to the first-install default seed. Reading never errors out
/// loud: a corrupt sidecar degrades to "seed defaults" rather than crashing.
pub fn load_relay_config(data_dir: &Path) -> Vec<(String, String)> {
    let path = data_dir.join(RELAY_CONFIG_FILENAME);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(entries) = serde_json::from_str::<Vec<RelayEntry>>(&content) else {
        return Vec::new();
    };
    entries.into_iter().map(|e| (e.url, e.role)).collect()
}

#[cfg(test)]
#[path = "relay_config_tests.rs"]
mod tests;
