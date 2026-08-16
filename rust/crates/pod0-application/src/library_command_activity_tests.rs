use pod0_domain::{CommandId, PodcastId, StateRevision};

use crate::{
    ActivityFact, ActivitySubject, DomainTransitionKind, LibraryCommandActivityInput,
    LibraryCommandMutation, LibraryFeedTransition, RequestDisposition, plan_library_command,
};

#[test]
fn state_change_has_typed_library_fact_and_subject() {
    let podcast_id = PodcastId::from_parts(0, 2);
    let plan = plan_library_command(LibraryCommandActivityInput {
        command_id: CommandId::from_parts(0, 1),
        subject: ActivitySubject::Podcast { podcast_id },
        episode_id: None,
        current_revision: StateRevision::new(8),
        legacy_command_revision: None,
        transition: LibraryFeedTransition::SubscriptionChanged,
        semantic_change: true,
    })
    .expect("plan library command");
    let (_, _, mutation, facts, _, _, disposition) = plan.into_parts();
    assert_eq!(mutation, LibraryCommandMutation::Apply);
    assert_eq!(disposition, RequestDisposition::Accepted);
    assert!(facts.iter().any(|draft| {
        draft.subject == ActivitySubject::Podcast { podcast_id }
            && matches!(
                draft.fact,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::LibraryFeed(
                        LibraryFeedTransition::SubscriptionChanged
                    ),
                    ..
                }
            )
    }));
}

#[test]
fn no_change_is_visible_without_a_false_transition() {
    let plan = plan_library_command(LibraryCommandActivityInput {
        command_id: CommandId::from_parts(0, 3),
        subject: ActivitySubject::Global,
        episode_id: None,
        current_revision: StateRevision::new(8),
        legacy_command_revision: None,
        transition: LibraryFeedTransition::NotificationPreferenceChanged,
        semantic_change: false,
    })
    .expect("plan no-change");
    let (_, _, mutation, facts, _, _, disposition) = plan.into_parts();
    assert_eq!(mutation, LibraryCommandMutation::RecordNoChange);
    assert_eq!(disposition, RequestDisposition::NoSemanticChange);
    assert_eq!(facts.len(), 1);
}
