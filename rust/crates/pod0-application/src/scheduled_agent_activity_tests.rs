use pod0_domain::{
    CancellationId, CommandId, ContentDigest, HostRequestId, ScheduledAttemptId,
    ScheduledOccurrenceId, StateRevision, UnixTimestampMilliseconds,
};

use crate::{
    ActivityFact, ActivitySubject, ExternalEffectKind, RequestDisposition,
    ScheduledAgentActivityTransition, ScheduledCommandActivityInput, ScheduledEffectAuthorization,
    plan_scheduled_command,
};

#[test]
fn scheduled_attempt_authorizes_provider_in_the_same_plan_without_private_prompt_content() {
    let occurrence_id = ScheduledOccurrenceId::from_parts(1, 2);
    let plan = plan_scheduled_command(ScheduledCommandActivityInput {
        command_id: CommandId::from_parts(3, 4),
        current_revision: StateRevision::new(8),
        committed_revision: StateRevision::new(9),
        disposition: RequestDisposition::Accepted,
        transitions: vec![(
            ActivitySubject::ScheduledOccurrence { occurrence_id },
            ScheduledAgentActivityTransition::AttemptStateChanged,
        )],
        effects: vec![ScheduledEffectAuthorization {
            request: crate::DurableScheduledAgentEffectRequest {
                request_id: HostRequestId::from_parts(5, 6),
                command_id: CommandId::from_parts(3, 4),
                cancellation_id: CancellationId::from_parts(7, 8),
                issued_revision: StateRevision::new(9),
                deadline_at: UnixTimestampMilliseconds::new(100),
                execution: crate::ScheduledAgentExecutionRequest {
                    occurrence_id,
                    attempt_id: ScheduledAttemptId::from_parts(9, 10),
                    prompt_revision: ContentDigest::from_bytes([1; 32]),
                    prompt: "private prompt".into(),
                    model_reference: "private model".into(),
                    context: Vec::new(),
                    maximum_output_bytes: 10,
                },
            },
        }],
        superseded_effects: Vec::new(),
    })
    .unwrap();
    let (_, _, _, facts, effects, _, _) = plan.into_parts();
    assert_eq!(effects.len(), 1);
    assert_eq!(
        effects[0].request.kind,
        ExternalEffectKind::ScheduledAgentProvider
    );
    assert_eq!(
        effects[0].request.subject,
        ActivitySubject::ScheduledOccurrence { occurrence_id }
    );
    assert!(facts.iter().any(|fact| matches!(
        fact.fact,
        ActivityFact::EffectAuthorized {
            kind: ExternalEffectKind::ScheduledAgentProvider,
            ..
        }
    )));
    let journal = serde_json::to_string(&facts.into_vec()).unwrap();
    assert!(!journal.contains("prompt"));
    assert!(!journal.contains("model"));
}
