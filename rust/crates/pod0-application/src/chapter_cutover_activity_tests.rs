use pod0_domain::{CommandId, StateRevision};

use crate::{
    ActivityDomain, ActivityFact, ChapterCutoverActivityInput, ChapterCutoverMutation,
    RequestDisposition, RequestRejectionReason, plan_chapter_cutover,
};

#[test]
fn accepted_cutover_emits_typed_transition_and_authority_fact() {
    let plan = plan_chapter_cutover(ChapterCutoverActivityInput {
        command_id: CommandId::from_parts(13, 1),
        current_revision: StateRevision::new(3),
        committed_revision: StateRevision::new(4),
        disposition: RequestDisposition::Accepted,
    })
    .unwrap();
    let (_, _, mutation, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(mutation, ChapterCutoverMutation::Apply);
    assert_eq!(facts.len(), 3);
    assert!(effects.is_empty() && commands.is_empty());
    assert!(matches!(
        facts.get(2).unwrap().fact,
        ActivityFact::AuthorityCutover {
            domain: ActivityDomain::Chapter
        }
    ));
}

#[test]
fn rejected_cutover_records_only_disposition() {
    let plan = plan_chapter_cutover(ChapterCutoverActivityInput {
        command_id: CommandId::from_parts(13, 2),
        current_revision: StateRevision::new(3),
        committed_revision: StateRevision::new(3),
        disposition: RequestDisposition::Rejected {
            reason: RequestRejectionReason::RevisionConflict,
        },
    })
    .unwrap();
    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, ChapterCutoverMutation::None);
    assert_eq!(facts.len(), 1);
}
