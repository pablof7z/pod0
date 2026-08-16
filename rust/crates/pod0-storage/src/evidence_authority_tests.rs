use pod0_application::{ActivityFact, RequestDisposition};
use pod0_domain::{CommandId, ContentDigest};

use crate::evidence_store_test_support::{EvidenceFixture, artifact};
use crate::{ActivityStore, EvidenceStore, StorageError};

#[test]
fn rebuild_is_atomic_replay_safe_and_emits_private_bounded_facts() {
    let fixture = EvidenceFixture::new();
    let artifact = artifact("private-source-revision");
    let episode_id = artifact.version.episode_id;
    let command_id = CommandId::from_parts(91, 1);
    let fingerprint = ContentDigest::from_bytes([0x91; 32]);
    let before = activity_count(&fixture);

    assert_eq!(
        crate::transition_commit::commit_evidence_rebuild_with_observer(
            &fixture.import.target,
            command_id,
            fingerprint,
            &artifact,
            None,
            1_800_000_000_091,
            || Err(StorageError::Interrupted),
        ),
        Err(StorageError::Interrupted)
    );
    assert_eq!(activity_count(&fixture), before);
    assert_eq!(
        fixture.store.selected_generation(episode_id).unwrap(),
        None
    );

    let revision = crate::transition_commit::commit_evidence_rebuild(
        &fixture.import.target,
        command_id,
        fingerprint,
        &artifact,
        None,
        1_800_000_000_092,
    )
    .unwrap();
    let page = ActivityStore::open(&fixture.import.target)
        .unwrap()
        .page_for_episode(episode_id, None, 100)
        .unwrap();
    assert_eq!(page.items.len(), before + 2);
    assert!(matches!(
        page.items[before].draft.fact,
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Accepted
        }
    ));
    let encoded = serde_json::to_string(&page.items).unwrap();
    assert!(!encoded.contains("Small habits become durable"));
    assert!(!encoded.contains("private-source-revision"));

    assert_eq!(
        crate::transition_commit::commit_evidence_rebuild(
            &fixture.import.target,
            command_id,
            fingerprint,
            &artifact,
            None,
            1_800_000_000_099,
        )
        .unwrap(),
        revision
    );
    assert_eq!(activity_count(&fixture), before + 2);
    assert_eq!(
        EvidenceStore::open(&fixture.import.target)
            .unwrap()
            .selected_artifact(episode_id)
            .unwrap(),
        Some(artifact)
    );
}

fn activity_count(fixture: &EvidenceFixture) -> usize {
    ActivityStore::open(&fixture.import.target)
        .unwrap()
        .page_for_episode(
            pod0_domain::EpisodeId::from_bytes([0x22; 16]),
            None,
            100,
        )
        .unwrap()
        .items
        .len()
}
