use super::test_support::*;
use crate::scheduled_agent_facade::qualify_scheduled_agent_completion;
use crate::*;

#[test]
fn commands_projection_and_completion_are_rust_owned_end_to_end() {
    let fixture = authoritative_fixture(1_000);
    let facade = open_scheduled(&fixture, 1_000);
    dispatch_scheduled(
        &facade,
        1,
        ApplicationCommand::EnsureScheduledTask {
            task: task_input(1_000),
        },
    );
    let projection = scheduled_projection(&facade);
    assert_eq!(projection.failure, None);
    assert_eq!(projection.tasks.len(), 1);
    assert_eq!(
        projection.tasks[0].prompt,
        "Summarize my saved podcast evidence"
    );

    dispatch_scheduled(&facade, 2, ApplicationCommand::ReconcileScheduledRuns);
    let request = facade.next_host_requests(20).pop().unwrap();
    let HostRequest::ExecuteScheduledAgentTurn { execution } = &request.request else {
        panic!("expected scheduled-agent host request")
    };
    let accepted = ScheduledAgentExecutionObservation::Accepted {
        occurrence_id: execution.occurrence_id,
        attempt_id: execution.attempt_id,
        provider_operation_id: None,
    };
    assert_eq!(
        facade.record_host_observation(scheduled_observation(&request, 0, 1_001, accepted,)),
        HostObservationReceipt::Persisted {
            request_id: request.request_id,
            terminal: false,
        }
    );
    assert_eq!(
        scheduled_projection(&facade).workflows[0].stage,
        ScheduledAgentStage::HostAccepted
    );

    let completed = qualify_scheduled_agent_completion(
        execution.clone(),
        "A bounded evidence briefing".to_owned(),
    )
    .unwrap();
    assert_eq!(
        facade.record_host_observation(scheduled_observation(
            &request,
            1,
            1_002,
            completed.clone(),
        )),
        HostObservationReceipt::Persisted {
            request_id: request.request_id,
            terminal: true,
        }
    );
    let projection = scheduled_projection(&facade);
    assert_eq!(
        projection.workflows[0].stage,
        ScheduledAgentStage::Succeeded
    );
    assert_eq!(
        projection.tasks[0].last_run_at,
        Some(UnixTimestampMilliseconds::new(1_002))
    );
    assert_eq!(
        projection.tasks[0].next_run_at,
        UnixTimestampMilliseconds::new(86_401_002)
    );

    let revision = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::ScheduledAgent { task_id: None },
            offset: 0,
            max_items: 20,
        })
        .state_revision;
    let duplicate =
        facade.record_host_observation(scheduled_observation(&request, 1, 1_003, completed));
    assert!(matches!(duplicate, HostObservationReceipt::Rejected { .. }));
    assert_eq!(
        facade
            .snapshot(ProjectionRequest {
                scope: ProjectionScope::ScheduledAgent { task_id: None },
                offset: 0,
                max_items: 20,
            })
            .state_revision,
        revision
    );
}

#[test]
fn requested_restart_reissues_exactly_once_and_accepted_restart_is_ambiguous() {
    let fixture = authoritative_fixture(2_000);
    let first = open_scheduled(&fixture, 2_000);
    dispatch_scheduled(
        &first,
        1,
        ApplicationCommand::EnsureScheduledTask {
            task: task_input(2_000),
        },
    );
    dispatch_scheduled(&first, 2, ApplicationCommand::ReconcileScheduledRuns);
    let original = first.next_host_requests(20).pop().unwrap();
    drop(first);

    let second = open_scheduled(&fixture, 2_001);
    let reissued = second.next_host_requests(20);
    assert_eq!(reissued, vec![original.clone()]);
    assert!(second.next_host_requests(20).is_empty());
    let HostRequest::ExecuteScheduledAgentTurn { execution } = &original.request else {
        panic!("expected scheduled-agent host request")
    };
    let accepted = ScheduledAgentExecutionObservation::Accepted {
        occurrence_id: execution.occurrence_id,
        attempt_id: execution.attempt_id,
        provider_operation_id: None,
    };
    assert!(matches!(
        second.record_host_observation(scheduled_observation(&original, 0, 2_002, accepted,)),
        HostObservationReceipt::Persisted {
            terminal: false,
            ..
        }
    ));
    drop(second);

    let third = open_scheduled(&fixture, 2_003);
    assert!(third.next_host_requests(20).is_empty());
    let projection = scheduled_projection(&third);
    assert_eq!(
        projection.workflows[0].stage,
        ScheduledAgentStage::Ambiguous
    );
    assert_eq!(
        projection.workflows[0]
            .failure
            .as_ref()
            .map(|failure| failure.code),
        Some(ScheduledAgentFailureCode::UnsafeToRetry)
    );
}

#[test]
fn cancellation_withdraws_exact_work_and_late_completion_cannot_commit() {
    let fixture = authoritative_fixture(3_000);
    let facade = open_scheduled(&fixture, 3_000);
    dispatch_scheduled(
        &facade,
        1,
        ApplicationCommand::EnsureScheduledTask {
            task: task_input(3_000),
        },
    );
    dispatch_scheduled(&facade, 2, ApplicationCommand::ReconcileScheduledRuns);
    let request = facade.next_host_requests(20).pop().unwrap();
    let workflow = scheduled_projection(&facade).workflows.remove(0);
    dispatch_scheduled(
        &facade,
        3,
        ApplicationCommand::CancelScheduledRun {
            occurrence_id: workflow.occurrence_id,
            expected_workflow_revision: workflow.workflow_revision,
        },
    );
    assert!(facade.next_host_requests(20).is_empty());
    assert_eq!(facade.next_host_cancellations(20).len(), 1);
    assert_eq!(
        scheduled_projection(&facade).workflows[0].stage,
        ScheduledAgentStage::Cancelled
    );

    let HostRequest::ExecuteScheduledAgentTurn { execution } = &request.request else {
        panic!("expected scheduled-agent host request")
    };
    let completion = qualify_scheduled_agent_completion(
        execution.clone(),
        "This callback arrived too late".to_owned(),
    )
    .unwrap();
    assert!(matches!(
        facade.record_host_observation(scheduled_observation(&request, 0, 3_001, completion,)),
        HostObservationReceipt::Rejected { .. }
    ));
    assert_eq!(
        scheduled_projection(&facade).workflows[0].stage,
        ScheduledAgentStage::Cancelled
    );
}

#[test]
fn explicit_retry_rearms_blocked_occurrence_and_issues_next_attempt() {
    let fixture = authoritative_fixture(4_000);
    let first = open_scheduled(&fixture, 4_000);
    dispatch_scheduled(
        &first,
        1,
        ApplicationCommand::EnsureScheduledTask {
            task: task_input(4_000),
        },
    );
    dispatch_scheduled(&first, 2, ApplicationCommand::ReconcileScheduledRuns);
    let request = first.next_host_requests(20).pop().unwrap();
    let HostRequest::ExecuteScheduledAgentTurn { execution } = &request.request else {
        panic!("expected scheduled-agent host request")
    };
    let blocked = ScheduledAgentExecutionObservation::Failed {
        occurrence_id: execution.occurrence_id,
        attempt_id: execution.attempt_id,
        code: ScheduledAgentFailureCode::MissingCredential,
        safe_detail: Some("Credential unavailable".to_owned()),
        retry_after_milliseconds: None,
    };
    assert!(matches!(
        first.record_host_observation(scheduled_observation(&request, 0, 4_001, blocked)),
        HostObservationReceipt::Persisted { terminal: true, .. }
    ));
    drop(first);

    let second = open_scheduled(&fixture, 4_002);
    let workflow = scheduled_projection(&second).workflows.remove(0);
    assert_eq!(workflow.stage, ScheduledAgentStage::Blocked);
    assert!(workflow.allowed_actions.can_retry);
    dispatch_scheduled(
        &second,
        3,
        ApplicationCommand::RetryScheduledRun {
            occurrence_id: workflow.occurrence_id,
            expected_workflow_revision: workflow.workflow_revision,
        },
    );
    assert_eq!(
        scheduled_projection(&second).workflows[0].stage,
        ScheduledAgentStage::RetryScheduled
    );
    dispatch_scheduled(&second, 4, ApplicationCommand::ReconcileScheduledRuns);
    let retried = second.next_host_requests(20).pop().unwrap();
    let HostRequest::ExecuteScheduledAgentTurn { execution } = retried.request else {
        panic!("expected retried scheduled-agent request")
    };
    assert_eq!(
        execution.attempt_id,
        scheduled_projection(&second).workflows[0]
            .attempt_id
            .unwrap()
    );
    assert_eq!(scheduled_projection(&second).workflows[0].attempt, 2);
}
