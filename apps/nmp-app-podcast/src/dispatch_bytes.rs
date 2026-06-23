//! ADR-0064 typed dispatch seam — the podcast-app bytes doorway.
//!
//! The JSON doorway `nmp_app_dispatch_action(app, namespace, json)` was deleted.
//! Every write now travels the typed [`nmp_ffi::nmp_app_dispatch_action_bytes`]
//! doorway: a host-minted `correlation_id` + the module's NAMESPACE + a typed
//! [`ActionPayload`](nmp_core::substrate::ActionPayload) payload, wrapped in a
//! [`DispatchEnvelope`](nmp_core::dispatch_envelope).
//!
//! This module encodes both NMP-owned namespaces (`nmp.publish`, `nmp.blossom.upload`)
//! and podcast-specific namespaces via `PodcastJsonPayload` pass-through.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;
use serde_json::Value;

use nmp_core::dispatch_envelope::{encode_dispatch_envelope, DISPATCH_ENVELOPE_SCHEMA_VERSION};
use nmp_core::substrate::ActionPayload;
use nmp_ffi::{nmp_app_dispatch_action_bytes, nmp_free_string, NmpApp};

/// Process-local correlation-id source.
///
/// The byte lane echoes a HOST-supplied `correlation_id` verbatim — unlike the
/// retired JSON lane, where the kernel minted it. The podcast app carries no
/// `uuid`/`rand` dependency, and a write correlation id only has to be unique
/// within one running process for the lifetime of an in-flight operation. A
/// monotone atomic counter satisfies that exactly. The `podcast-` prefix
/// namespaces it so it never collides with the kernel's hex correlation ids.
static NEXT_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

/// Mint a fresh process-local correlation id for a byte-doorway dispatch.
#[must_use]
pub fn mint_correlation_id() -> String {
    let n = NEXT_CORRELATION_ID.fetch_add(1, Ordering::Relaxed);
    format!("podcast-{n}")
}

/// Encode `json` into the typed [`ActionPayload`] FlatBuffers bytes for `namespace`.
///
/// `namespace` is the module's HOST namespace (e.g. `nmp.publish`, `podcast.player`).
/// For NMP-owned namespaces, this uses their typed `ActionPayload` impls.
/// For podcast namespaces, this uses the `PodcastJsonPayload` pass-through.
/// Returns a fail-closed error string for an unknown namespace or a body that
/// does not deserialize into the namespace's typed action.
fn encode_payload_for_namespace(namespace: &str, json: &str) -> Result<Vec<u8>, String> {
    match namespace {
        "nmp.publish" => encode::<nmp_core::publish::PublishAction>(namespace, json),
        "nmp.blossom.upload" => encode::<nmp_blossom::UploadInput>(namespace, json),
        // Podcast-specific namespaces: wrap raw JSON in the pass-through payload.
        // PodcastJsonPayload is not serde-Deserializable (it wraps opaque JSON),
        // so we construct it directly instead of going through the generic encode<P>.
        ns if ns.starts_with("podcast.") => {
            let payload = crate::action_payload::PodcastJsonPayload {
                schema_version: crate::action_payload::PodcastJsonPayload::SCHEMA_VERSION,
                body_json: json.to_owned(),
            };
            Ok(payload.encode())
        }
        other => Err(format!(
            "no typed payload encoder for action namespace '{other}' (byte doorway has no JSON fallback)"
        )),
    }
}

/// Deserialize `json` into `P` and encode it to typed [`ActionPayload`] bytes.
fn encode<P>(namespace: &str, json: &str) -> Result<Vec<u8>, String>
where
    P: ActionPayload + DeserializeOwned,
{
    let action: P = serde_json::from_str(json).map_err(|e| {
        format!("action body for '{namespace}' does not match its typed payload shape: {e}")
    })?;
    Ok(action.encode())
}

/// Dispatch a podcast action through the typed byte doorway.
///
/// Builds the typed payload for `namespace` from `json`, mints a host
/// correlation id, wraps payload + namespace + id in an open [`DispatchEnvelope`],
/// and hands the finished bytes to [`nmp_app_dispatch_action_bytes`]. Returns
/// the echoed correlation id on accept, or a fail-closed error string on a
/// null app, an unknown / mis-shaped namespace, or a kernel rejection.
///
/// # Safety
/// `app` must be a valid non-null `*mut NmpApp` from `nmp_app_new` (a null `app`
/// returns an error string, never a crash).
pub fn dispatch_action_bytes_for(
    app: *mut NmpApp,
    namespace: &str,
    json: &str,
) -> Result<String, String> {
    if app.is_null() {
        return Err("runtime app is not available".to_string());
    }

    let payload = encode_payload_for_namespace(namespace, json)?;
    let correlation_id = mint_correlation_id();

    let envelope = encode_dispatch_envelope(
        &correlation_id,
        namespace,
        DISPATCH_ENVELOPE_SCHEMA_VERSION,
        &payload,
    );

    // SAFETY: envelope is valid bytes produced by encode_dispatch_envelope,
    // app is non-null, correlation_id is a valid C string.
    let result_ptr = unsafe { nmp_app_dispatch_action_bytes(app, envelope.as_ptr(), envelope.len()) };

    if result_ptr.is_null() {
        return Err("kernel rejected the action".to_string());
    }

    // SAFETY: result_ptr is from the kernel's malloc; we take ownership via CString.
    let result_json = unsafe {
        let c_str = std::ffi::CStr::from_ptr(result_ptr as *const i8);
        c_str.to_string_lossy().to_string()
    };

    // SAFETY: result_ptr is heap-owned from the kernel; free it.
    unsafe {
        nmp_free_string(result_ptr);
    }

    // Parse the returned JSON to extract correlation_id or error
    match serde_json::from_str::<Value>(&result_json) {
        Ok(Value::Object(map)) => {
            if let Some(Value::String(id)) = map.get("correlation_id") {
                Ok(id.clone())
            } else if let Some(Value::String(err)) = map.get("error") {
                Err(err.clone())
            } else {
                Ok(correlation_id)
            }
        }
        _ => Ok(correlation_id),
    }
}
