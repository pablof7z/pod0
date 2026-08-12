use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EffectAttemptId, EffectIntentId, EpisodeId,
    HostRequestId, StateRevision, TranscriptWorkflowId,
};

use crate::{
    ActivityFact, EffectOutcome, RequestDisposition, TranscriptObservationActivityInput,
    TranscriptTransition, plan_transcript_observation,
};

#[test]
fn effect_observation_preserves_authorization_causation_and_transition() {
    let cause = ActivityId::from_parts(1, 1);
    let intent = EffectIntentId::from_parts(2, 2);
    let attempt = EffectAttemptId::from_parts(3, 3);
    let plan = plan_transcript_observation(TranscriptObservationActivityInput {
        command_id: CommandId::from_parts(4, 4),
        request_id: HostRequestId::from_parts(5, 5),
        episode_id: EpisodeId::from_parts(6, 6),
        workflow_id: TranscriptWorkflowId::from_parts(7, 7),
        workflow_revision: StateRevision::new(8),
        intent_id: intent,
        attempt_id: attempt,
        authorizing_activity_id: cause,
        correlation_id: ActivityCorrelationId::from_parts(9, 9),
        outcome: EffectOutcome::Succeeded,
        transition: TranscriptTransition::AttemptStateChanged,
    })
    .unwrap();

    assert_eq!(plan.disposition(), RequestDisposition::Accepted);
    let (_, _, _, facts, effects, commands, _) = plan.into_parts();
    assert!(effects.is_empty());
    assert!(commands.is_empty());
    assert!(
        facts
            .iter()
            .all(|fact| fact.caused_by_activity_id == Some(cause))
    );
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::EffectObserved {
            intent_id,
            attempt_id,
            outcome: EffectOutcome::Succeeded,
        } if intent_id == intent && attempt_id == attempt
    ));
}
