use pod0_application::{ApplicationCommand, ScheduledTaskInput};
use sha2::{Digest as _, Sha256};

pub(super) fn hash_scheduled_agent_command(hash: &mut Sha256, command: &ApplicationCommand) {
    match command {
        ApplicationCommand::EnsureScheduledTask { task } => {
            hash.update(b"ensure-scheduled-task\0");
            hash_task(hash, task);
        }
        ApplicationCommand::UpdateScheduledTask {
            task_id,
            expected_task_revision,
            task,
        } => {
            hash.update(b"update-scheduled-task\0");
            hash.update(task_id.into_bytes());
            hash.update(expected_task_revision.value.to_be_bytes());
            hash_task(hash, task);
        }
        ApplicationCommand::RemoveScheduledTask {
            task_id,
            expected_task_revision,
        } => {
            hash.update(b"remove-scheduled-task\0");
            hash.update(task_id.into_bytes());
            hash.update(expected_task_revision.value.to_be_bytes());
        }
        ApplicationCommand::ReconcileScheduledRuns => hash.update(b"reconcile-scheduled-runs\0"),
        ApplicationCommand::CancelScheduledRun {
            occurrence_id,
            expected_workflow_revision,
        } => {
            hash.update(b"cancel-scheduled-run\0");
            hash.update(occurrence_id.into_bytes());
            hash.update(expected_workflow_revision.value.to_be_bytes());
        }
        _ => unreachable!("scheduled-agent fingerprint called for another command"),
    }
}

fn hash_task(hash: &mut Sha256, task: &ScheduledTaskInput) {
    match task.task_id {
        Some(task_id) => {
            hash.update([1]);
            hash.update(task_id.into_bytes());
        }
        None => hash.update([0]),
    }
    hash_text(hash, &task.label);
    hash_text(hash, &task.prompt);
    hash_text(hash, &task.model_reference);
    hash.update(task.interval_milliseconds.to_be_bytes());
    hash.update(task.next_run_at.value.to_be_bytes());
}

fn hash_text(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
}
