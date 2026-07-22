use pod0_application::{
    ScheduledAgentExecutionObservation, ScheduledAgentFailureCode, ScheduledAgentStage,
    scheduled_generated_artifact_id,
};
use pod0_domain::{ContentDigest, GeneratedArtifactId};
use rusqlite::Connection;

use crate::scheduled_agent_store_test_support::{ScheduledFixture, time};
use crate::*;

#[test]
fn task_reconcile_and_requested_restart_are_durable_and_idempotent() {
    let fixture = ScheduledFixture::new();
    let definition = fixture.definition(1, 2_000);
    let ensure = fixture.context(1, definition.created_at.value());
    assert!(matches!(
        fixture
            .store
            .ensure_task(ensure, definition.clone())
            .unwrap(),
        ScheduledTaskMutationOutcome::Applied(_)
    ));
    assert!(matches!(
        fixture
            .store
            .ensure_task(ensure, definition.clone())
            .unwrap(),
        ScheduledTaskMutationOutcome::Duplicate(_)
    ));

    let reconcile = fixture.context(2, 2_000);
    let first = fixture.store.reconcile_due_runs(reconcile).unwrap();
    assert_eq!(first.created_occurrences.len(), 1);
    assert_eq!(first.requests.len(), 1);
    let request = first.requests[0].clone();
    let duplicate = fixture.store.reconcile_due_runs(reconcile).unwrap();
    assert_eq!(duplicate.requests, vec![request.clone()]);

    let reopened = ScheduledAgentStore::open_authoritative(&fixture.path).unwrap();
    let recovery = reopened.recover_after_restart(time(2_100)).unwrap();
    assert_eq!(recovery.reissued_requests, vec![request]);
    assert!(recovery.ambiguous_occurrences.is_empty());
}

#[test]
fn accepted_restart_fails_closed_as_ambiguous_without_resubmission() {
    let fixture = ScheduledFixture::new();
    let definition = fixture.definition(1, 2_000);
    fixture
        .store
        .ensure_task(
            fixture.context(1, definition.created_at.value()),
            definition,
        )
        .unwrap();
    let request = fixture
        .store
        .reconcile_due_runs(fixture.context(2, 2_000))
        .unwrap()
        .requests
        .remove(0);
    let outcome = fixture
        .store
        .apply_observation(ScheduledAgentObservationInput {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 1,
            observed_at: time(2_100),
            observation: ScheduledAgentExecutionObservation::Accepted {
                occurrence_id: request.execution.occurrence_id,
                attempt_id: request.execution.attempt_id,
                provider_operation_id: Some("provider-operation".to_owned()),
            },
        })
        .unwrap();
    assert!(matches!(
        outcome,
        ScheduledAgentObservationOutcome::Updated(_)
    ));

    let recovery = fixture.store.recover_after_restart(time(2_200)).unwrap();
    assert!(recovery.reissued_requests.is_empty());
    assert_eq!(
        recovery.ambiguous_occurrences,
        vec![request.execution.occurrence_id]
    );
    let state = fixture
        .store
        .occurrence(request.execution.occurrence_id)
        .unwrap()
        .unwrap();
    assert_eq!(state.stage, ScheduledAgentStage::Ambiguous);
    assert_eq!(
        state.failure.unwrap().code,
        ScheduledAgentFailureCode::UnsafeToRetry
    );
}

#[test]
fn completion_atomically_selects_artifact_and_advances_recurrence_once() {
    let fixture = ScheduledFixture::new();
    let definition = fixture.definition(1, 2_000);
    fixture
        .store
        .ensure_task(
            fixture.context(1, definition.created_at.value()),
            definition.clone(),
        )
        .unwrap();
    let request = fixture
        .store
        .reconcile_due_runs(fixture.context(2, 2_000))
        .unwrap()
        .requests
        .remove(0);
    let artifact_id = scheduled_generated_artifact_id(request.execution.attempt_id);
    let digest = ContentDigest::from_bytes([7; 32]);
    let completion = ScheduledAgentObservationInput {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 1,
        observed_at: time(2_500),
        observation: ScheduledAgentExecutionObservation::Completed {
            occurrence_id: request.execution.occurrence_id,
            attempt_id: request.execution.attempt_id,
            artifact_id,
            output_digest: digest,
            output_excerpt: "A bounded generated briefing".to_owned(),
        },
    };
    let outcome = fixture.store.apply_observation(completion.clone()).unwrap();
    let ScheduledAgentObservationOutcome::Updated(state) = outcome else {
        panic!("updated")
    };
    assert_eq!(state.stage, ScheduledAgentStage::Succeeded);
    let task = fixture.store.task(definition.task_id).unwrap().unwrap();
    assert_eq!(task.last_run_at, Some(time(2_500)));
    assert_eq!(task.next_run_at, time(2_500 + 86_400_000));
    assert!(matches!(
        fixture.store.apply_observation(completion).unwrap(),
        ScheduledAgentObservationOutcome::Duplicate(_)
    ));
    let connection = Connection::open(&fixture.path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM pod0_generated_artifacts),\
         (SELECT COUNT(*) FROM pod0_scheduled_completion_evidence WHERE state='committed')",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .unwrap(),
        (1, 1)
    );
}

#[test]
fn noncanonical_completion_rolls_back_occurrence_artifact_and_recurrence() {
    let fixture = ScheduledFixture::new();
    let shared_artifact = GeneratedArtifactId::from_parts(6, 1);
    complete_task(&fixture, 1);

    let definition = fixture.definition(2, 4_000);
    fixture
        .store
        .ensure_task(
            fixture.context(3, definition.created_at.value()),
            definition.clone(),
        )
        .unwrap();
    let request = fixture
        .store
        .reconcile_due_runs(fixture.context(4, 4_000))
        .unwrap()
        .requests
        .remove(0);
    let result = fixture
        .store
        .apply_observation(ScheduledAgentObservationInput {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 1,
            observed_at: time(4_500),
            observation: ScheduledAgentExecutionObservation::Completed {
                occurrence_id: request.execution.occurrence_id,
                attempt_id: request.execution.attempt_id,
                artifact_id: shared_artifact,
                output_digest: ContentDigest::from_bytes([9; 32]),
                output_excerpt: "Conflicting artifact".to_owned(),
            },
        });
    assert_eq!(result, Err(StorageError::ScheduledAgentWorkflowConflict));
    assert_eq!(
        fixture
            .store
            .occurrence(request.execution.occurrence_id)
            .unwrap()
            .unwrap()
            .stage,
        ScheduledAgentStage::Requested
    );
    let task = fixture.store.task(definition.task_id).unwrap().unwrap();
    assert_eq!(task.last_run_at, None);
    assert_eq!(task.next_run_at, time(4_000));
}

fn complete_task(fixture: &ScheduledFixture, value: u64) {
    let due = 2_000 + i64::try_from(value).unwrap();
    let definition = fixture.definition(value, due);
    fixture
        .store
        .ensure_task(
            fixture.context(value, definition.created_at.value()),
            definition,
        )
        .unwrap();
    let request = fixture
        .store
        .reconcile_due_runs(fixture.context(value + 10, due))
        .unwrap()
        .requests
        .remove(0);
    fixture
        .store
        .apply_observation(ScheduledAgentObservationInput {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 1,
            observed_at: time(due + 100),
            observation: ScheduledAgentExecutionObservation::Completed {
                occurrence_id: request.execution.occurrence_id,
                attempt_id: request.execution.attempt_id,
                artifact_id: scheduled_generated_artifact_id(request.execution.attempt_id),
                output_digest: ContentDigest::from_bytes([8; 32]),
                output_excerpt: "First artifact".to_owned(),
            },
        })
        .unwrap();
}
