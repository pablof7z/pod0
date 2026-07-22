use pod0_domain::{
    CancellationId, CommandId, ContentDigest, ScheduledTaskId, StateRevision,
    UnixTimestampMilliseconds,
};

use super::*;

fn plan() -> ScheduledAgentAttemptPlan {
    let prompt = "Prepare a briefing".to_owned();
    let definition = ScheduledTaskDefinition {
        task_id: ScheduledTaskId::from_parts(4, 8),
        label: "Daily briefing".to_owned(),
        prompt_revision: scheduled_prompt_revision(&prompt).unwrap(),
        prompt,
        model_reference: "openrouter:test/model".to_owned(),
        interval_milliseconds: 86_400_000,
        created_at: UnixTimestampMilliseconds::new(1_000),
        last_run_at: None,
        next_run_at: UnixTimestampMilliseconds::new(1_000),
        revision: StateRevision::new(1),
    };
    let occurrence =
        reconcile_scheduled_occurrence(&definition, UnixTimestampMilliseconds::new(1_000))
            .unwrap()
            .unwrap();
    begin_scheduled_agent_attempt(&occurrence, UnixTimestampMilliseconds::new(1_000)).unwrap()
}

#[test]
fn host_ledger_keeps_acceptance_streaming_then_retires_completion() {
    let plan = plan();
    let mut ledger = HostRequestLedger::default();
    let envelope = HostRequestEnvelope {
        request_id: plan.request_id,
        command_id: CommandId::from_parts(1, 1),
        cancellation_id: CancellationId::from_parts(2, 2),
        issued_revision: plan.state.revision,
        deadline_at: Some(plan.deadline_at),
        request: HostRequest::ExecuteScheduledAgentTurn {
            execution: plan.request.clone(),
        },
    };
    assert!(ledger.register(envelope.clone()));
    let observation = |sequence_number, value| HostObservationEnvelope {
        request_id: envelope.request_id,
        cancellation_id: envelope.cancellation_id,
        observed_request_revision: envelope.issued_revision,
        sequence_number,
        observed_at: UnixTimestampMilliseconds::new(2_000 + sequence_number as i64),
        observation: HostObservation::ScheduledAgentExecutionObserved { observation: value },
    };
    assert_eq!(
        ledger.accept_observation(&observation(
            1,
            ScheduledAgentExecutionObservation::Accepted {
                occurrence_id: plan.request.occurrence_id,
                attempt_id: plan.request.attempt_id,
                provider_operation_id: None,
            }
        )),
        ObservationAcceptance::Accepted
    );
    let completion = ScheduledAgentExecutionObservation::Completed {
        occurrence_id: plan.request.occurrence_id,
        attempt_id: plan.request.attempt_id,
        artifact_id: scheduled_generated_artifact_id(plan.request.attempt_id),
        output_digest: ContentDigest::from_bytes([3; 32]),
        output_excerpt: "Complete".to_owned(),
    };
    assert_eq!(
        ledger.accept_observation(&observation(2, completion.clone())),
        ObservationAcceptance::Accepted
    );
    assert_eq!(
        ledger.accept_observation(&observation(2, completion)),
        ObservationAcceptance::Duplicate
    );
}
