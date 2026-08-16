use pod0_domain::{CommandId, EpisodeId, HostRequestId, StateRevision, TranscriptWorkflowId};

use crate::{
    ActivityActor, ActivityFact, ActivityOrigin, TranscriptAmbiguousRecoveryInput,
    plan_transcript_ambiguous_recovery,
};

#[test]
fn ambiguous_submission_recovery_is_typed_and_bounded() {
    let plan = plan_transcript_ambiguous_recovery(TranscriptAmbiguousRecoveryInput {
        recovery_id: CommandId::from_parts(1, 1),
        command_id: CommandId::from_parts(1, 2),
        request_id: HostRequestId::from_parts(1, 3),
        episode_id: EpisodeId::from_parts(1, 4),
        workflow_id: TranscriptWorkflowId::from_parts(1, 5),
        current_revision: StateRevision::new(8),
    })
    .unwrap();
    let (_, expected, (), facts, effects, commands, _) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(8));
    assert_eq!(facts.len(), 2);
    assert!(effects.is_empty() && commands.is_empty());
    assert!(facts.iter().all(
        |fact| fact.actor == ActivityActor::Recovery && fact.origin == ActivityOrigin::Recovery
    ));
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition {
            committed_revision: StateRevision { value: 9 },
            ..
        }
    ));
}
