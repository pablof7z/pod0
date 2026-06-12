//! Unit tests for [`super::decode_action_results_sidecar`].
//!
//! Tests are linked via `#[path]` in `snapshot_action_results.rs`.

use nmp_core::{
    encode_snapshot_frame, SnapshotEnvelope, TypedProjectionData,
    typed_projections::{
        ACTION_RESULTS_SCHEMA_ID, ACTION_RESULTS_SCHEMA_VERSION,
    },
};

use super::decode_action_results_sidecar;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stub_envelope() -> SnapshotEnvelope {
    SnapshotEnvelope {
        rev: 1,
        running: true,
        ..SnapshotEnvelope::default()
    }
}

fn frame_with_typed(typed: &[TypedProjectionData]) -> Vec<u8> {
    encode_snapshot_frame(&stub_envelope(), typed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A frame without an `action_results` typed sidecar returns `None` —
/// silently absent (D6), never a crash.
#[test]
fn absent_sidecar_yields_none() {
    let frame = frame_with_typed(&[]);
    assert!(
        decode_action_results_sidecar(&frame).is_none(),
        "absent action_results sidecar must yield None, not a crash"
    );
}

/// A frame with a malformed `action_results` payload (empty bytes) also returns
/// `None` — D6: degrade silently, never panic.
#[test]
fn malformed_payload_yields_none() {
    let entry = TypedProjectionData {
        key: ACTION_RESULTS_SCHEMA_ID.to_string(),
        schema_id: ACTION_RESULTS_SCHEMA_ID.to_string(),
        schema_version: ACTION_RESULTS_SCHEMA_VERSION,
        file_identifier: "KARS".to_string(),
        payload: vec![],
    };
    let frame = frame_with_typed(&[entry]);
    assert!(
        decode_action_results_sidecar(&frame).is_none(),
        "empty/malformed payload must yield None (D6 degrade silently)"
    );
}

/// Golden `KARS` (action_results) FlatBuffer payload carrying ONE settled
/// `nmp.blossom.upload` row whose `result` is a serialised `BlobDescriptor`.
///
/// These bytes were emitted by the kernel's own `flatbuffers` builder against
/// the `action_results.fbs` schema embedded in nmp-core v0.6.0 (rev `4fdcb52d`).
/// `encode_action_results` is `pub(crate)` and the generated builder lives in a
/// private module, so the success-path fixture cannot be produced at test time;
/// it is captured here verbatim. The `decode_action_results` round-trip below
/// is the seam-contract anchor — if a future NMP schema bump invalidates these
/// bytes, this test fails loudly (which is the point: catch wire drift).
///
/// Decoded shape:
/// ```json
/// { "correlation_id": "blossom-test-corr-1", "status": "published",
///   "result": "{\"url\":\"https://blossom.example/abcd.png\", …}" }
/// ```
#[rustfmt::skip]
const KARS_BLOB_UPLOAD_FIXTURE: &[u8] = &[
    0x10, 0x00, 0x00, 0x00, 0x4b, 0x41, 0x52, 0x53, 0x00, 0x00, 0x06, 0x00, 0x08, 0x00, 0x04, 0x00,
    0x06, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00,
    0x10, 0x00, 0x14, 0x00, 0x08, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x10, 0x00,
    0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x80, 0x00, 0x00, 0x00, 0x6c, 0x00, 0x00, 0x00,
    0x04, 0x00, 0x00, 0x00, 0x5c, 0x00, 0x00, 0x00, 0x7b, 0x22, 0x75, 0x72, 0x6c, 0x22, 0x3a, 0x22,
    0x68, 0x74, 0x74, 0x70, 0x73, 0x3a, 0x2f, 0x2f, 0x62, 0x6c, 0x6f, 0x73, 0x73, 0x6f, 0x6d, 0x2e,
    0x65, 0x78, 0x61, 0x6d, 0x70, 0x6c, 0x65, 0x2f, 0x61, 0x62, 0x63, 0x64, 0x2e, 0x70, 0x6e, 0x67,
    0x22, 0x2c, 0x22, 0x73, 0x68, 0x61, 0x32, 0x35, 0x36, 0x22, 0x3a, 0x22, 0x61, 0x62, 0x63, 0x64,
    0x22, 0x2c, 0x22, 0x73, 0x69, 0x7a, 0x65, 0x22, 0x3a, 0x31, 0x32, 0x33, 0x34, 0x2c, 0x22, 0x75,
    0x70, 0x6c, 0x6f, 0x61, 0x64, 0x65, 0x64, 0x22, 0x3a, 0x31, 0x37, 0x30, 0x30, 0x30, 0x30, 0x30,
    0x30, 0x30, 0x30, 0x7d, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x70, 0x75, 0x62, 0x6c,
    0x69, 0x73, 0x68, 0x65, 0x64, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00, 0x62, 0x6c, 0x6f, 0x73,
    0x73, 0x6f, 0x6d, 0x2d, 0x74, 0x65, 0x73, 0x74, 0x2d, 0x63, 0x6f, 0x72, 0x72, 0x2d, 0x31, 0x00,
];

/// SUCCESS-PATH SEAM CONTRACT: a frame carrying a real `KARS` sidecar with one
/// settled `nmp.blossom.upload` row must decode to a JSON array Swift's
/// `ActionResultsRegistry` can drain — `action_results[correlation_id].result`
/// must carry the `BlobDescriptor` with the `url` reachable.
///
/// This closes the bridge-decode gap that has silently broken before: it proves
/// the Rust → FFI-JSON → (Swift-reachable) `url` path end-to-end on the Rust
/// side. Swift's drain is covered by `ActionResultsRegistry` unit tests.
#[test]
fn blob_upload_result_round_trips_to_reachable_url() {
    // Sanity: the fixture itself is a valid KARS buffer (guards against the
    // embedded bytes rotting against a schema bump).
    let model = nmp_core::typed_projections::decode_action_results(KARS_BLOB_UPLOAD_FIXTURE)
        .expect("embedded KARS fixture must decode against the pinned schema");
    assert_eq!(model.results.len(), 1, "fixture carries exactly one row");

    // Wrap the payload as the kernel does and run the FFI decode bridge.
    let entry = TypedProjectionData {
        key: ACTION_RESULTS_SCHEMA_ID.to_string(),
        schema_id: ACTION_RESULTS_SCHEMA_ID.to_string(),
        schema_version: ACTION_RESULTS_SCHEMA_VERSION,
        file_identifier: "KARS".to_string(),
        payload: KARS_BLOB_UPLOAD_FIXTURE.to_vec(),
    };
    let frame = frame_with_typed(&[entry]);

    let decoded =
        decode_action_results_sidecar(&frame).expect("non-empty KARS sidecar must decode to Some");
    let arr = decoded.as_array().expect("decode yields a JSON array");
    assert_eq!(arr.len(), 1, "one settled row");

    let row = &arr[0];
    assert_eq!(
        row["correlation_id"], "blossom-test-corr-1",
        "correlation_id is the registry drain key"
    );
    assert_eq!(row["status"], "published");
    assert!(row.get("error").is_none(), "success row carries no error");

    // `result` is the serialised BlobDescriptor JSON string the registry parses.
    let result_str = row["result"]
        .as_str()
        .expect("result is a serialised JSON string (forwarded verbatim, D0)");
    let blob: serde_json::Value =
        serde_json::from_str(result_str).expect("result parses as BlobDescriptor JSON");
    assert_eq!(
        blob["url"], "https://blossom.example/abcd.png",
        "the BlobDescriptor url must be reachable by the awaiting Swift caller"
    );
}
