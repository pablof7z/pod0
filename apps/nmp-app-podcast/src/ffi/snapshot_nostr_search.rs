//! Decode bridge for NMP NIP-50 search-result typed sidecars.
//!
//! `nmp_app_intent_dispatch` opens NIP-50 searches through the app-owned facade
//! and registers one typed sidecar per session under `nmp.nip50.search.<session_id>`.
//! This bridge decodes those generic NMP sidecars into JSON so the Swift shell
//! can observe search sessions from the normal push-frame path without knowing
//! FlatBuffers.

const SEARCH_KEY_PREFIX: &str = "nmp.nip50.search.";

/// Decode all NIP-50 search-result sidecars from a raw update-frame slice.
///
/// Returns a map keyed by the original projection key
/// (`nmp.nip50.search.<session_id>`) with a JSON value shaped like
/// `SearchResultsSnapshot`. Absent or malformed sidecars are silently skipped
/// (D6).
pub(super) fn decode_nostr_search_sidecars(
    slice: &[u8],
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let typed = nmp_core::decode_snapshot_typed_projections(slice).ok()?;
    let mut map = serde_json::Map::new();
    for entry in typed {
        if entry.schema_id != nmp_nip50::SEARCH_RESULTS_SCHEMA_ID {
            continue;
        }
        if !entry.key.starts_with(SEARCH_KEY_PREFIX) {
            continue;
        }
        let Ok(snapshot) = nmp_nip50::decode_search_results_snapshot(&entry.payload) else {
            continue;
        };
        if let Ok(value) = serde_json::to_value(snapshot) {
            map.insert(entry.key, value);
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

#[cfg(test)]
#[path = "snapshot_nostr_search_tests.rs"]
mod tests;
