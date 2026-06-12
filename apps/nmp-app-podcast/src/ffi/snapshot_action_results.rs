//! Decode bridge for the `action_results` typed FlatBuffer sidecar.
//!
//! Extracted from `snapshot.rs` to keep that file under the 500-line AGENTS.md
//! hard limit. `snapshot.rs` calls [`decode_action_results_sidecar`] and injects
//! the return value into `v.projections["action_results"]`.
//!
//! Wire shape per `action_results_fb.rs` (nmp-core):
//! ```json
//! [
//!   { "correlation_id": "…", "status": "published", "result": "{…}" },
//!   { "correlation_id": "…", "status": "failed",    "error":  "…"  }
//! ]
//! ```
//!
//! Swift's `ActionResultsRegistry` drains entries keyed by `correlation_id`
//! to resolve the awaiting `BlobDescriptor` (from `nmp.blossom.upload`) or any
//! other async-completing action result. Decode failure degrades silently
//! (D6 — key absent, never a crash).

/// Decode the `action_results` typed FlatBuffer sidecar from a raw update-frame
/// slice and convert it to the JSON array Swift reads from
/// `v.projections["action_results"]`.
///
/// Returns `None` when the sidecar is absent, empty, or malformed (D6).
pub(super) fn decode_action_results_sidecar(slice: &[u8]) -> Option<serde_json::Value> {
    use nmp_core::typed_projections::{decode_action_results, ACTION_RESULTS_SCHEMA_ID};

    let typed = nmp_core::decode_snapshot_typed_projections(slice).ok()?;
    let entry = typed
        .into_iter()
        .find(|e| e.schema_id == ACTION_RESULTS_SCHEMA_ID)?;
    let model = decode_action_results(&entry.payload).ok()?;
    if model.results.is_empty() {
        return None;
    }
    let arr: Vec<serde_json::Value> = model
        .results
        .into_iter()
        .map(|row| {
            let mut obj = serde_json::json!({
                "correlation_id": row.correlation_id,
                "status": row.status,
            });
            if let Some(err) = row.error {
                obj["error"] = serde_json::Value::String(err);
            }
            if let Some(res) = row.result {
                obj["result"] = serde_json::Value::String(res);
            }
            obj
        })
        .collect();
    Some(serde_json::Value::Array(arr))
}

#[cfg(test)]
#[path = "snapshot_action_results_tests.rs"]
mod tests;
