//! Unit tests for the NIP-50 search sidecar decode bridge.

use nmp_core::{encode_snapshot_frame, SnapshotEnvelope, TypedProjectionData};
use nmp_nip50::{
    encode_search_results_snapshot, SearchHit, SearchHitSource, SearchResultsSnapshot,
    SEARCH_RESULTS_SCHEMA_ID, SEARCH_RESULTS_SCHEMA_VERSION,
};

use super::decode_nostr_search_sidecars;

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

#[test]
fn absent_sidecar_yields_none() {
    let frame = frame_with_typed(&[]);
    assert!(decode_nostr_search_sidecars(&frame).is_none());
}

#[test]
fn ignores_non_search_sidecars() {
    let entry = TypedProjectionData {
        key: "podcast.library".to_string(),
        schema_id: "podcast.library".to_string(),
        payload: br#"{"rev":1}"#.to_vec(),
        ..Default::default()
    };
    let frame = frame_with_typed(&[entry]);
    assert!(decode_nostr_search_sidecars(&frame).is_none());
}

#[test]
fn search_snapshot_round_trips_to_json_by_session_key() {
    let snapshot = SearchResultsSnapshot {
        hits: vec![SearchHit {
            id: "11".repeat(32),
            author: "22".repeat(32),
            kind: 0,
            created_at: 1_700_000_001,
            content: r#"{"name":"Alice","picture":"https://example.com/a.png"}"#.to_string(),
            tags: vec![vec!["p".to_string(), "33".repeat(32)]],
            relay_provenance: vec!["wss://search.example/".to_string()],
            source: SearchHitSource::Relay("wss://search.example/".to_string()),
        }],
    };
    let entry = TypedProjectionData {
        key: "nmp.nip50.search.ios-discover-1".to_string(),
        schema_id: SEARCH_RESULTS_SCHEMA_ID.to_string(),
        schema_version: SEARCH_RESULTS_SCHEMA_VERSION,
        file_identifier: "N50S".to_string(),
        payload: encode_search_results_snapshot(&snapshot),
        ..Default::default()
    };
    let frame = frame_with_typed(&[entry]);
    let map = decode_nostr_search_sidecars(&frame).expect("search sidecar");
    let value = map
        .get("nmp.nip50.search.ios-discover-1")
        .expect("session key");
    assert_eq!(value["hits"][0]["author"], "22".repeat(32));
    assert_eq!(value["hits"][0]["source"]["Relay"], "wss://search.example/");
}
