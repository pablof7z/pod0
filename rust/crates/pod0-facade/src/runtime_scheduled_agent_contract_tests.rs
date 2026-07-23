use crate::runtime_command_fingerprint::command_fingerprint;
use crate::*;

fn task(label: &str, next_run_at: i64) -> ScheduledTaskInput {
    ScheduledTaskInput {
        task_id: Some(ScheduledTaskId::from_parts(1, 2)),
        label: label.to_owned(),
        prompt: "Prepare a daily briefing".to_owned(),
        model_reference: "openrouter:test/model".to_owned(),
        interval_milliseconds: 86_400_000,
        next_run_at: UnixTimestampMilliseconds::new(next_run_at),
    }
}

#[test]
fn scheduled_agent_contract_is_typed_but_unavailable_before_storage_slice() {
    let facade = Pod0Facade::new();
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(0, 1),
        cancellation_id: CancellationId::from_parts(0, 2),
        expected_revision: None,
        command: ApplicationCommand::EnsureScheduledTask {
            task: task("Daily", 1_000),
        },
    });
    let snapshot = facade.snapshot(ProjectionRequest {
        scope: ProjectionScope::ScheduledAgent { task_id: None },
        offset: 0,
        max_items: 20,
    });
    assert_eq!(snapshot.contract_version, 40);
    let Projection::ScheduledAgent { value } = snapshot.projection else {
        panic!("expected scheduled-agent projection");
    };
    assert!(value.tasks.is_empty());
    assert!(value.workflows.is_empty());
    assert_eq!(
        value.failure.map(|failure| failure.code),
        Some(CoreFailureCode::StorageUnavailable)
    );

    let Projection::Library { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected library projection");
    };
    assert!(value.operations.iter().any(|operation| {
        operation.command_id == CommandId::from_parts(0, 1)
            && operation.stage == OperationStage::Failed
            && operation.failure.as_ref().map(|failure| failure.code)
                == Some(CoreFailureCode::StorageUnavailable)
    }));
}

#[test]
fn scheduled_agent_command_fingerprints_cover_every_durable_input() {
    let first = ApplicationCommand::EnsureScheduledTask {
        task: task("Daily", 1_000),
    };
    assert_eq!(command_fingerprint(&first), command_fingerprint(&first));
    assert_ne!(
        command_fingerprint(&first),
        command_fingerprint(&ApplicationCommand::EnsureScheduledTask {
            task: task("Weekly", 1_000),
        })
    );
    assert_ne!(
        command_fingerprint(&first),
        command_fingerprint(&ApplicationCommand::EnsureScheduledTask {
            task: task("Daily", 1_001),
        })
    );
    let mut boundary_left = task("Daily", 1_000);
    boundary_left.prompt = "a".to_owned();
    boundary_left.model_reference = "bc".to_owned();
    let mut boundary_right = boundary_left.clone();
    boundary_right.prompt = "ab".to_owned();
    boundary_right.model_reference = "c".to_owned();
    assert_ne!(
        command_fingerprint(&ApplicationCommand::EnsureScheduledTask {
            task: boundary_left,
        }),
        command_fingerprint(&ApplicationCommand::EnsureScheduledTask {
            task: boundary_right,
        })
    );

    let cancel = |revision| ApplicationCommand::CancelScheduledRun {
        occurrence_id: ScheduledOccurrenceId::from_parts(4, 5),
        expected_workflow_revision: StateRevision::new(revision),
    };
    assert_ne!(
        command_fingerprint(&cancel(1)),
        command_fingerprint(&cancel(2))
    );
    assert_ne!(
        command_fingerprint(&cancel(1)),
        command_fingerprint(&ApplicationCommand::CancelScheduledRun {
            occurrence_id: ScheduledOccurrenceId::from_parts(4, 6),
            expected_workflow_revision: StateRevision::new(1),
        })
    );
}
