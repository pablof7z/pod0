use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityActor, ActivityDomain, ActivityFact, ActivityOrigin, RequestDisposition,
    RequestRejectionReason, TranscriptCutoverActivityInput, TranscriptCutoverMutation,
    plan_transcript_cutover,
};

#[test]
fn accepted_cutover_has_only_bounded_migration_facts() {
    let plan = plan_transcript_cutover(TranscriptCutoverActivityInput {
        command_id: CommandId::from_parts(21, 1),
        current_revision: StateRevision::new(4),
        committed_revision: StateRevision::new(5),
        disposition: RequestDisposition::Accepted,
    })
    .unwrap();
    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, TranscriptCutoverMutation::Apply);
    assert_eq!(facts.len(), 3);
    assert!(facts.iter().all(|item| {
        item.actor == ActivityActor::Migration && item.origin == ActivityOrigin::Migration
    }));
    assert!(matches!(
        facts.get(2).unwrap().fact,
        ActivityFact::AuthorityCutover {
            domain: ActivityDomain::Transcript
        }
    ));
}

#[test]
fn rejected_cutover_is_durable_without_mutation() {
    let plan = plan_transcript_cutover(TranscriptCutoverActivityInput {
        command_id: CommandId::from_parts(21, 2),
        current_revision: StateRevision::new(4),
        committed_revision: StateRevision::new(4),
        disposition: RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        },
    })
    .unwrap();
    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, TranscriptCutoverMutation::None);
    assert_eq!(facts.len(), 1);
}
