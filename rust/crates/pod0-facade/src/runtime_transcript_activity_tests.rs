use pod0_application::{
    ActivityFact, DomainTransitionKind, RequestDisposition, RequestRejectionReason,
    TranscriptTransition,
};
use pod0_domain::{CommandId, StateRevision};
use pod0_storage::ActivityStore;

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::runtime_transcript_tests::{envelope, input};

#[test]
fn transcript_ingestion_activity_is_atomic_complete_and_replay_safe() {
    let fixture = PlaybackFixture::new();
    let accepted = envelope(1, StateRevision::INITIAL, input(&fixture, "source-v1"));
    fixture.facade.dispatch(accepted.clone());

    let accepted_activity = activity(&fixture);
    assert!(accepted_activity.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Transcript(TranscriptTransition::ArtifactAdopted),
            ..
        }
    )));
    assert!(accepted_activity.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Transcript(TranscriptTransition::SelectionChanged),
            ..
        }
    )));
    let accepted_fact_count = accepted_activity.len();
    fixture.facade.dispatch(accepted);
    assert_eq!(activity(&fixture).len(), accepted_fact_count);

    fixture.facade.dispatch(envelope(
        2,
        StateRevision::INITIAL,
        input(&fixture, "stale-source"),
    ));
    let mut invalid = input(&fixture, "invalid-source");
    invalid.segments[0].end_milliseconds = 0;
    fixture
        .facade
        .dispatch(envelope(3, StateRevision::new(1), invalid));

    let rejected_activity = activity(&fixture);
    assert!(has_rejection(
        &rejected_activity,
        2,
        RequestRejectionReason::RevisionConflict,
    ));
    assert!(has_rejection(
        &rejected_activity,
        3,
        RequestRejectionReason::Invalid,
    ));
}

fn activity(fixture: &PlaybackFixture) -> Vec<pod0_application::CommittedActivityFact> {
    ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap()
        .items
}

fn has_rejection(
    activity: &[pod0_application::CommittedActivityFact],
    command_suffix: u64,
    expected: RequestRejectionReason,
) -> bool {
    activity.iter().any(|item| {
        matches!(
            item.draft.fact,
            ActivityFact::RequestDisposition {
                disposition: RequestDisposition::Rejected { reason },
            } if reason == expected
                && item.draft.command_id == Some(CommandId::from_parts(60, command_suffix))
        )
    })
}
