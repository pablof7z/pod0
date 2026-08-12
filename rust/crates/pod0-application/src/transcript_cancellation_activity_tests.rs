use pod0_domain::{CommandId, EpisodeId, StateRevision, TranscriptWorkflowId};

use crate::{
    ActivityFact, DomainTransitionKind, RequestDisposition, TranscriptCancellationActivityInput,
    TranscriptTransition, plan_transcript_cancellation,
};

#[test]
fn cancellation_is_a_typed_state_transition_without_an_external_effect() {
    let plan = plan_transcript_cancellation(TranscriptCancellationActivityInput {
        command_id: CommandId::from_parts(1, 2),
        episode_id: EpisodeId::from_parts(3, 4),
        workflow_id: TranscriptWorkflowId::from_parts(5, 6),
        workflow_revision: StateRevision::new(7),
    })
    .unwrap();

    assert_eq!(plan.disposition(), RequestDisposition::Accepted);
    let (_, expected, _, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(7));
    assert!(effects.is_empty());
    assert!(commands.is_empty());
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Transcript(TranscriptTransition::Cancelled),
            previous_revision,
            committed_revision,
        } if previous_revision == StateRevision::new(7)
            && committed_revision == StateRevision::new(8)
    ));
}
