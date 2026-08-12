use pod0_domain::{CommandId, NoteId, StateRevision};

use crate::{
    ActivityFact, ActivitySubject, RequestDisposition, RequestRejectionReason,
    UserArtifactActivityInput, UserArtifactMutation, UserArtifactTransition,
    plan_user_artifact_activity,
};

#[test]
fn accepted_artifact_mutation_has_one_domain_transition_without_episode_links() {
    let plan = plan_user_artifact_activity(UserArtifactActivityInput {
        command_id: CommandId::from_parts(1, 2),
        subject: ActivitySubject::Note {
            note_id: NoteId::from_parts(3, 4),
        },
        episode_ids: Vec::new(),
        current_revision: StateRevision::new(7),
        committed_revision: StateRevision::new(10),
        transition: UserArtifactTransition::NoteChanged,
        disposition: RequestDisposition::Accepted,
    })
    .unwrap();
    let (_, expected, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(7));
    assert_eq!(mutation, UserArtifactMutation::Apply);
    assert_eq!(facts.len(), 2);
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition { .. }
    ));
}

#[test]
fn rejected_artifact_request_is_a_fact_without_a_mutation() {
    let plan = plan_user_artifact_activity(UserArtifactActivityInput {
        command_id: CommandId::from_parts(1, 2),
        subject: ActivitySubject::Global,
        episode_ids: Vec::new(),
        current_revision: StateRevision::new(7),
        committed_revision: StateRevision::new(7),
        transition: UserArtifactTransition::MemoryChanged,
        disposition: RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        },
    })
    .unwrap();
    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, UserArtifactMutation::None);
    assert_eq!(facts.len(), 1);
}
