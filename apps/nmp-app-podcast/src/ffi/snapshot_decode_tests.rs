//! Tests for [`super::decode_signed_events_sidecar`] — the bridge that injects
//! the typed FlatBuffer `signed_events` sidecar into `v.projections` so the
//! iOS `SignedEventsRegistry.ingest` path continues to work after the v0.3.0
//! typed-first migration.
//!
//! Tests are in a separate file (linked via `#[path]` in `snapshot.rs`) so
//! `snapshot.rs` stays under the 500-line AGENTS.md hard limit.

use nmp_core::{
    encode_snapshot_frame, SnapshotEnvelope, TypedProjectionData,
    typed_projections::SIGNED_EVENTS_SCHEMA_ID,
};

use super::decode_signed_events_sidecar;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal [`SnapshotEnvelope`] adequate for encoding a fixture frame.
fn stub_envelope() -> SnapshotEnvelope {
    SnapshotEnvelope {
        rev: 1,
        running: true,
        ..SnapshotEnvelope::default()
    }
}

/// Build a wire frame carrying the supplied typed sidecar entries.
fn frame_with_typed(typed: &[TypedProjectionData]) -> Vec<u8> {
    encode_snapshot_frame(&stub_envelope(), typed)
}

/// Produce a frame that carries a real `signed_events` typed sidecar.
///
/// Spins up the kernel actor, starts it (so frames emit), then issues a
/// `SignEventForReturn` with an empty `account_pubkey` (active account
/// convention). With no account registered the kernel resolves the sign
/// synchronously with an `Err("no active account")` result — which STILL
/// produces a `signed_events` sidecar entry (`ok: false, error: ...`).
/// This path is sufficient to validate that the sidecar decode + inject
/// code path is exercised end-to-end.
///
/// Returns the raw wire frame bytes.
fn frame_with_live_signed_events() -> Vec<u8> {
    use nmp_core::testing::{spawn_actor, wait_barrier, ActorCommand};
    use nmp_core::decode_snapshot_typed_projections;

    let (tx, rx) = spawn_actor();

    // Start the actor (sets running=true so periodic frames emit).
    tx.send(ActorCommand::Start {
        visible_limit: 80,
        emit_hz: 4,
        initial_relays: vec![],
    })
    .expect("actor reachable");

    // Issue a SignEventForReturn with an empty pubkey (active account
    // convention). No account is registered so the kernel resolves this
    // immediately with ok=false — sufficient to produce a signed_events sidecar.
    let unsigned = serde_json::json!({
        "kind": 1,
        "content": "test sign-and-return",
        "tags": [],
        "created_at": 1_700_000_000u64,
    })
    .to_string();
    tx.send(ActorCommand::SignEventForReturn {
        account_pubkey: String::new(), // empty → active account
        unsigned_json: unsigned,
        correlation_id: "test-corr-id".to_string(),
    })
    .expect("actor reachable");

    // Barrier: ensure the SignEventForReturn command is processed before we
    // start draining frames.
    let synced = wait_barrier(&tx, std::time::Duration::from_secs(10));
    assert!(synced, "Barrier must ack after SignEventForReturn");

    // Drain frames until the signed_events sidecar appears (up to 15 s).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "signed_events sidecar never appeared within deadline"
        );
        let Ok(frame) = rx.recv_timeout(std::time::Duration::from_millis(300)) else {
            continue;
        };
        let Ok(typed) = decode_snapshot_typed_projections(&frame) else {
            continue;
        };
        if typed.iter().any(|e| e.schema_id == SIGNED_EVENTS_SCHEMA_ID) {
            return frame;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A frame without a typed sidecar produces `None` from the helper —
/// silently absent (D6), never a crash.
#[test]
fn absent_sidecar_yields_none() {
    let frame = frame_with_typed(&[]);
    assert!(
        decode_signed_events_sidecar(&frame).is_none(),
        "absent signed_events sidecar must yield None, not a crash"
    );
}

/// A frame with a malformed `signed_events` payload (zero bytes) also produces
/// `None` — D6 degrade silently, never panic.
#[test]
fn malformed_payload_yields_none() {
    use nmp_core::typed_projections::SIGNED_EVENTS_SCHEMA_VERSION;

    let entry = TypedProjectionData {
        key: SIGNED_EVENTS_SCHEMA_ID.to_string(),
        schema_id: SIGNED_EVENTS_SCHEMA_ID.to_string(),
        schema_version: SIGNED_EVENTS_SCHEMA_VERSION,
        file_identifier: "KSEV".to_string(),
        payload: vec![],
        ..Default::default()
    };
    let frame = frame_with_typed(&[entry]);
    assert!(
        decode_signed_events_sidecar(&frame).is_none(),
        "empty/malformed payload must yield None (D6 degrade silently)"
    );
}

/// A frame with a real `signed_events` typed sidecar (produced by a live
/// kernel sign-and-return) populates `v.projections["signed_events"]` with
/// the expected JSON shape. We use the error path (no active account) which
/// produces `{ correlation_id: { "ok": false, "error": "..." } }` — sufficient
/// to verify the full decode + JSON shape is correct.
#[test]
fn signed_events_sidecar_is_injected_with_expected_json_shape() {
    let frame = frame_with_live_signed_events();

    let result = decode_signed_events_sidecar(&frame)
        .expect("signed_events sidecar must be present in the frame");

    let obj = result
        .as_object()
        .expect("signed_events JSON must be an object keyed by correlation_id");

    let entry = obj
        .get("test-corr-id")
        .expect("must carry our correlation id");

    // The "ok" bool must be present (true or false depending on the kernel result).
    assert!(
        entry.get("ok").and_then(serde_json::Value::as_bool).is_some(),
        "entry must carry an \"ok\" bool"
    );

    // No-active-account path: ok=false, error field present.
    // If an account happened to be active (edge case), ok=true, signed_json present.
    let ok = entry["ok"].as_bool().unwrap();
    if ok {
        assert!(
            entry.get("signed_json").and_then(serde_json::Value::as_str).is_some(),
            "ok=true entry must carry a signed_json string"
        );
    } else {
        assert!(
            entry.get("error").and_then(serde_json::Value::as_str).is_some(),
            "ok=false entry must carry an error string"
        );
    }
}

/// Golden: a frame without a typed sidecar results in no `projections` key
/// in the `nmp_app_podcast_decode_update_frame` JSON output — the existing
/// steady-state behavior is preserved.
#[test]
fn frame_without_signed_events_has_no_projections_key_in_output() {
    let frame = frame_with_typed(&[]);
    let json = unsafe {
        let ptr = super::nmp_app_podcast_decode_update_frame(frame.as_ptr(), frame.len());
        assert!(!ptr.is_null(), "frame must decode successfully");
        let s = std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .into_owned();
        let _ = std::ffi::CString::from_raw(ptr);
        s
    };
    let v: serde_json::Value = serde_json::from_str(&json).expect("output must be valid JSON");
    assert_eq!(v["t"], "snapshot");
    assert!(
        v["v"].get("projections").is_none(),
        "frame without signed_events must not carry a projections key in the output"
    );
}

/// Faithful port of Swift's `JSONDecoder.KeyDecodingStrategy.convertFromSnakeCase`
/// (the algorithm the bridge decoder is configured with). Splits on `_`,
/// lowercases the leading run, and capitalizes the first letter of each
/// subsequent component. Mirrors the stdlib reference implementation closely
/// enough for our ASCII field names.
fn convert_from_snake_case(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let components: Vec<&str> = key.split('_').collect();
    // Leading/trailing empty components (from leading/trailing underscores) are
    // preserved by the stdlib; our keys have none, so a simple filter is safe.
    let non_empty: Vec<&str> = components.iter().copied().filter(|c| !c.is_empty()).collect();
    if non_empty.is_empty() {
        return key.to_string();
    }
    let mut out = String::new();
    out.push_str(&non_empty[0].to_lowercase());
    for comp in &non_empty[1..] {
        let mut chars = comp.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// Sanity: the convertFromSnakeCase port matches a few known Swift conversions.
#[test]
fn convert_from_snake_case_matches_swift_reference_cases() {
    assert_eq!(convert_from_snake_case("auto_download_mode"), "autoDownloadMode");
    assert_eq!(convert_from_snake_case("is_subscribed"), "isSubscribed");
    assert_eq!(convert_from_snake_case("rev"), "rev");
    assert_eq!(convert_from_snake_case("schema_version"), "schemaVersion");
}

/// Integration: a frame WITH a live `signed_events` sidecar results in
/// `v.projections["signed_events"]` being present in the
/// `nmp_app_podcast_decode_update_frame` output JSON — the iOS
/// `SignedEventsRegistry.ingest` path reads exactly this location.
#[test]
fn frame_with_signed_events_populates_projections_in_output() {
    let frame = frame_with_live_signed_events();
    let json = unsafe {
        let ptr = super::nmp_app_podcast_decode_update_frame(frame.as_ptr(), frame.len());
        assert!(!ptr.is_null(), "frame must decode successfully");
        let s = std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .into_owned();
        let _ = std::ffi::CString::from_raw(ptr);
        s
    };
    let v: serde_json::Value = serde_json::from_str(&json).expect("output must be valid JSON");
    assert_eq!(v["t"], "snapshot");

    // iOS shell reads: raw["v"]["projections"]["signed_events"]
    let signed_events = &v["v"]["projections"]["signed_events"];
    assert!(
        !signed_events.is_null(),
        "v.projections.signed_events must be populated when the sidecar is present"
    );
    let obj = signed_events
        .as_object()
        .expect("signed_events must be a JSON object");
    assert!(
        obj.contains_key("test-corr-id"),
        "signed_events must contain the correlation id"
    );
}
