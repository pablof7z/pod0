use pod0_application::{
    ScheduledAgentExecutionObservation, ScheduledAgentFailureCode, ScheduledAgentStage,
};

use crate::scheduled_agent_store_test_support::{ScheduledFixture, time};
use crate::{
    ScheduledAgentObservationInput, ScheduledAgentObservationOutcome, ScheduledAgentStore,
    StorageError,
};

#[test]
fn retry_block_and_cancel_transitions_survive_reopen() {
    let fixture = ScheduledFixture::new();
    let network = start(&fixture, 1, 2_000);
    let failed = fixture
        .store
        .apply_observation(observation(
            &network,
            1,
            2_100,
            ScheduledAgentExecutionObservation::Failed {
                occurrence_id: network.execution.occurrence_id,
                attempt_id: network.execution.attempt_id,
                code: ScheduledAgentFailureCode::Network,
                safe_detail: Some("network unavailable".to_owned()),
                retry_after_milliseconds: Some(500),
            },
        ))
        .unwrap();
    let ScheduledAgentObservationOutcome::Updated(retry) = failed else {
        panic!("updated")
    };
    assert_eq!(retry.stage, ScheduledAgentStage::RetryScheduled);
    assert_eq!(retry.not_before, Some(time(2_600)));

    let early = fixture
        .store
        .reconcile_due_runs(fixture.context(10, 2_599))
        .unwrap();
    assert!(early.requests.is_empty());
    let second = fixture
        .store
        .reconcile_due_runs(fixture.context(11, 2_600))
        .unwrap();
    assert_eq!(second.requests.len(), 1);
    assert_eq!(
        second.requests[0].execution.occurrence_id,
        network.execution.occurrence_id
    );

    let cancelled = fixture
        .store
        .cancel_occurrence(
            fixture.context(12, 2_700),
            network.execution.occurrence_id,
            second_request_state(&fixture, &second.requests[0]).revision,
        )
        .unwrap();
    assert_eq!(cancelled.stage, ScheduledAgentStage::Cancelled);
    let reopened = ScheduledAgentStore::open_authoritative(&fixture.path).unwrap();
    assert_eq!(
        reopened
            .occurrence(network.execution.occurrence_id)
            .unwrap()
            .unwrap()
            .stage,
        ScheduledAgentStage::Cancelled
    );

    let blocked_request = start(&fixture, 2, 4_000);
    let blocked = fixture
        .store
        .apply_observation(observation(
            &blocked_request,
            1,
            4_100,
            ScheduledAgentExecutionObservation::Failed {
                occurrence_id: blocked_request.execution.occurrence_id,
                attempt_id: blocked_request.execution.attempt_id,
                code: ScheduledAgentFailureCode::MissingCredential,
                safe_detail: None,
                retry_after_milliseconds: None,
            },
        ))
        .unwrap();
    let ScheduledAgentObservationOutcome::Updated(blocked) = blocked else {
        panic!("updated")
    };
    assert_eq!(blocked.stage, ScheduledAgentStage::Blocked);
    assert!(blocked.failure.unwrap().retryable);
}

#[test]
fn stale_and_conflicting_observations_never_mutate_authority() {
    let fixture = ScheduledFixture::new();
    let request = start(&fixture, 1, 2_000);
    let accepted = observation(
        &request,
        7,
        2_100,
        ScheduledAgentExecutionObservation::Accepted {
            occurrence_id: request.execution.occurrence_id,
            attempt_id: request.execution.attempt_id,
            provider_operation_id: Some("provider-1".to_owned()),
        },
    );
    let first = fixture.store.apply_observation(accepted.clone()).unwrap();
    let ScheduledAgentObservationOutcome::Updated(expected) = first else {
        panic!("updated")
    };
    assert!(matches!(
        fixture.store.apply_observation(accepted).unwrap(),
        ScheduledAgentObservationOutcome::Duplicate(_)
    ));
    let conflict = observation(
        &request,
        7,
        2_101,
        ScheduledAgentExecutionObservation::Accepted {
            occurrence_id: request.execution.occurrence_id,
            attempt_id: request.execution.attempt_id,
            provider_operation_id: Some("provider-2".to_owned()),
        },
    );
    assert_eq!(
        fixture.store.apply_observation(conflict),
        Err(StorageError::ScheduledAgentWorkflowConflict)
    );
    let stale = observation(
        &request,
        6,
        2_102,
        ScheduledAgentExecutionObservation::Cancelled {
            occurrence_id: request.execution.occurrence_id,
            attempt_id: request.execution.attempt_id,
        },
    );
    assert!(matches!(
        fixture.store.apply_observation(stale).unwrap(),
        ScheduledAgentObservationOutcome::Stale
    ));
    assert_eq!(
        fixture
            .store
            .occurrence(request.execution.occurrence_id)
            .unwrap()
            .unwrap(),
        expected
    );
}

fn start(
    fixture: &ScheduledFixture,
    value: u64,
    due_ms: i64,
) -> crate::ScheduledAgentHostRequestRecord {
    let definition = fixture.definition(value, due_ms);
    fixture
        .store
        .ensure_task(
            fixture.context(value, definition.created_at.value()),
            definition,
        )
        .unwrap();
    fixture
        .store
        .reconcile_due_runs(fixture.context(value + 2, due_ms))
        .unwrap()
        .requests
        .remove(0)
}

fn observation(
    request: &crate::ScheduledAgentHostRequestRecord,
    sequence_number: u64,
    observed_at_ms: i64,
    observation: ScheduledAgentExecutionObservation,
) -> ScheduledAgentObservationInput {
    ScheduledAgentObservationInput {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number,
        observed_at: time(observed_at_ms),
        observation,
    }
}

fn second_request_state(
    fixture: &ScheduledFixture,
    request: &crate::ScheduledAgentHostRequestRecord,
) -> pod0_application::ScheduledAgentOccurrenceState {
    fixture
        .store
        .occurrence(request.execution.occurrence_id)
        .unwrap()
        .unwrap()
}
