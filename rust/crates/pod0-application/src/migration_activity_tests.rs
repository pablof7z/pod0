use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityFact, ActivityOrigin, RequestDisposition,
    UserArtifactMigrationActivityInput, UserArtifactMigrationMutation, UserArtifactTransition,
    plan_user_artifact_migration,
};

#[test]
fn accepted_cutover_is_a_migration_transition_and_authority_fact() {
    let plan = plan_user_artifact_migration(UserArtifactMigrationActivityInput {
        command_id: CommandId::from_parts(1, 9),
        current_revision: StateRevision::new(4),
        committed_revision: StateRevision::new(5),
        transition: UserArtifactTransition::NoteChanged,
        disposition: RequestDisposition::Accepted,
        authority_cutover: true,
    })
    .unwrap();
    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, UserArtifactMigrationMutation::Apply);
    assert_eq!(facts.len(), 3);
    assert_eq!(facts.get(0).unwrap().actor, ActivityActor::Migration);
    assert_eq!(facts.get(0).unwrap().origin, ActivityOrigin::Migration);
    assert!(matches!(
        facts.get(2).unwrap().fact,
        ActivityFact::AuthorityCutover { .. }
    ));
}

#[test]
fn rejected_migration_is_durable_without_mutation() {
    let plan = plan_user_artifact_migration(UserArtifactMigrationActivityInput {
        command_id: CommandId::from_parts(1, 10),
        current_revision: StateRevision::new(4),
        committed_revision: StateRevision::new(4),
        transition: UserArtifactTransition::ClipChanged,
        disposition: RequestDisposition::Rejected {
            reason: crate::RequestRejectionReason::RevisionConflict,
        },
        authority_cutover: false,
    })
    .unwrap();
    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, UserArtifactMigrationMutation::None);
    assert_eq!(facts.len(), 1);
}
