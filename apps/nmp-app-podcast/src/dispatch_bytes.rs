//! ADR-0064 typed dispatch seam — the podcast-app bytes doorway.
//!
//! The JSON doorway `nmp_app_dispatch_action(app, namespace, json)` was deleted.
//! Every write now travels the typed
//! [`nmp_native_runtime::dispatch_action_bytes_typed`] doorway (the
//! `nmp-ffi` C-ABI wrapper that used to own this call, including the
//! malloc'd-C-string round trip, is deleted — the runtime function returns a
//! typed [`DispatchOutcome`](nmp_native_runtime::action_dispatch::DispatchOutcome)
//! directly): a host-minted `correlation_id` + the module's NAMESPACE + a
//! typed [`ActionPayload`](nmp_core::substrate::ActionPayload) payload,
//! wrapped in a [`DispatchEnvelope`](nmp_core::dispatch_envelope).
//!
//! This module encodes both NMP-owned namespaces (`nmp.publish`, `nmp.blossom.upload`)
//! and podcast-specific namespaces via `PodcastJsonPayload` pass-through.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;

use nmp_core::dispatch_envelope::{encode_dispatch_envelope, DISPATCH_ENVELOPE_SCHEMA_VERSION};
use nmp_core::substrate::ActionPayload;
use nmp_native_runtime::{dispatch_action_bytes_typed, NmpApp};

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
        // Podcast-specific namespaces (bare "podcast" or "podcast.*" family):
        // wrap raw JSON in the pass-through payload. PodcastJsonPayload is not
        // serde-Deserializable (it wraps opaque JSON), so we construct it directly
        // instead of going through the generic encode<P>.
        ns if ns == "podcast" || ns.starts_with("podcast.") => {
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
/// and hands the finished bytes to
/// [`dispatch_action_bytes_typed`](nmp_native_runtime::dispatch_action_bytes_typed).
/// Returns the echoed correlation id on accept, or a fail-closed error
/// string on a null app, an unknown / mis-shaped namespace, or a kernel
/// rejection.
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

    // SAFETY: app is non-null (checked above) and owned by the host NMP
    // runtime for the duration of this call.
    let app_ref = unsafe { &*app };
    let outcome = dispatch_action_bytes_typed(app_ref, &envelope);

    if let Some(err) = outcome.error {
        return Err(err);
    }
    Ok(outcome.correlation_id.unwrap_or(correlation_id))
}
