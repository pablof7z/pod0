use pod0_application::{
    ActivityFact, ChapterTransition, DomainTransitionKind, RequestDisposition,
    RequestRejectionReason,
};
use pod0_storage::ActivityStore;

use crate::runtime_chapter_tests::{chapter_input, envelope};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn chapter_ingestion_activity_is_atomic_complete_and_replay_safe() {
    let fixture = PlaybackFixture::new_with_chapters();
    let accepted = envelope(40, chapter_input(&fixture, "Fresh chapter"), 1);
    fixture.facade.dispatch(accepted.clone());

    let accepted_activity = activity(&fixture);
    assert!(accepted_activity.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Chapter(ChapterTransition::ArtifactAdopted),
            ..
        }
    )));
    assert!(accepted_activity.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Chapter(ChapterTransition::SelectionChanged),
            ..
        }
    )));
    let accepted_count = accepted_activity.len();
    fixture.facade.dispatch(accepted);
    assert_eq!(activity(&fixture).len(), accepted_count);

    fixture
        .facade
        .dispatch(envelope(41, chapter_input(&fixture, "Stale chapter"), 1));
    let mut invalid = chapter_input(&fixture, "Invalid chapter");
    invalid.chapters.clear();
    fixture.facade.dispatch(envelope(42, invalid, 2));
    let rejected = activity(&fixture);
    assert!(has_rejection(
        &rejected,
        41,
        RequestRejectionReason::RevisionConflict,
    ));
    assert!(has_rejection(
        &rejected,
        42,
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
                && item.draft.command_id == Some(CommandId::from_parts(30, command_suffix))
        )
    })
}
