use super::*;
use pod0_application::{
    ActivityActor, ActivityFact, ActivityOrigin, CommandActivityIdentity,
};

fn activity(
    fixture: &TranscriptImportFixture,
    import_id: pod0_domain::CommandId,
) -> Vec<pod0_application::CommittedActivityFact> {
    let correlation = CommandActivityIdentity::new(import_id).correlation_id();
    crate::ActivityStore::open(&fixture.import.target)
        .unwrap()
        .page_for_correlation(correlation, None, 20)
        .unwrap()
        .items
}

#[test]
fn cutover_is_the_only_authority_flip_and_replay_is_exactly_once() {
    let fixture = TranscriptImportFixture::current();
    let import_id = command(71);
    fixture.stage(import_id).unwrap();
    fixture.verify(import_id).unwrap();
    assert!(activity(&fixture, import_id).is_empty());
    assert!(!crate::transcript_store_is_authoritative(
        &fixture.import.target
    )
    .unwrap());

    fixture.commit(import_id).unwrap();
    let facts = activity(&fixture, import_id);
    assert_eq!(facts.len(), 3);
    assert!(facts.iter().all(|item| {
        item.draft.actor == ActivityActor::Migration
            && item.draft.origin == ActivityOrigin::Migration
    }));
    assert!(matches!(
        facts[2].draft.fact,
        ActivityFact::AuthorityCutover { .. }
    ));
    assert!(crate::transcript_store_is_authoritative(
        &fixture.import.target
    )
    .unwrap());

    fixture.commit(import_id).unwrap();
    assert_eq!(activity(&fixture, import_id), facts);
    let payload: String = rusqlite::Connection::open(&fixture.import.target)
        .unwrap()
        .query_row(
            "SELECT GROUP_CONCAT(payload_json,'') FROM pod0_activity_facts \
             WHERE correlation_id=?1",
            [CommandActivityIdentity::new(import_id)
                .correlation_id()
                .into_bytes()
                .as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!payload.contains("Small habits become durable"));
    assert!(!payload.contains("Ada"));
}
