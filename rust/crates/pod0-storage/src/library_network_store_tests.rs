use pod0_domain::{CancellationId, CommandId, ContentDigest};

use crate::{commit_listening_cutover, library_store_tests::imported_fixture};

#[test]
fn library_network_admission_is_durable_exact_and_replay_safe() {
    let fixture = imported_fixture();
    assert!(!commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap());
    assert!(commit_listening_cutover(&fixture.target, 1_800_000_000_001).unwrap());
    let command_id = CommandId::from_parts(43, 1);
    let input = crate::LibraryNetworkAdmissionInput {
        command_id,
        cancellation_id: CancellationId::from_parts(43, 2),
        command_fingerprint: "a".repeat(64),
        fingerprint: ContentDigest::from_bytes([7; 32]),
        intent: pod0_application::LibraryNetworkIntent::DirectorySearch {
            query: "durable history".into(),
            limit: 8,
        },
        now_ms: 1_800_000_000_010,
        deadline_at_ms: 1_800_000_030_010,
    };
    let store = crate::LibraryStore::open_authoritative(&fixture.target).unwrap();
    let admitted = store.admit_library_network(input.clone()).unwrap();
    let request_id = admitted.pending_request_id.unwrap();
    let revision = admitted.revision;
    drop(store);

    let reopened = crate::LibraryStore::open_authoritative(&fixture.target).unwrap();
    let recovered = reopened
        .library_network_workflow(command_id)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.pending_request_id, Some(request_id));
    assert_eq!(recovered.revision, revision);
    let effect_count = library_effect_count(&fixture.target, command_id);
    assert_eq!(effect_count, 1);

    let replay = reopened.admit_library_network(input).unwrap();
    assert_eq!(replay, recovered);
    assert_eq!(
        library_effect_count(&fixture.target, command_id),
        effect_count
    );
}

fn library_effect_count(path: &std::path::Path, command_id: CommandId) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM pod0_effect_intents WHERE effect_kind_code=17 AND subject_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap()
}
