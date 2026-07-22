use pod0_application::{
    CoreFailureCode, ScheduledTaskDefinition, ScheduledTaskInput, scheduled_prompt_revision,
};
use pod0_domain::{ScheduledOccurrenceId, ScheduledTaskId, StateRevision};
use pod0_storage::{
    ScheduledAgentCommandContext, ScheduledAgentReconcileOutcome, ScheduledAgentStore, StorageError,
};

use super::command_fingerprint::scheduled_agent_command_fingerprint;
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;
use crate::{CommandEnvelope, CommandId};

impl FacadeState {
    pub(crate) fn accept_scheduled_agent_command(
        &mut self,
        envelope: &CommandEnvelope,
        command: pod0_application::ApplicationCommand,
    ) {
        match command {
            pod0_application::ApplicationCommand::EnsureScheduledTask { task } => {
                self.ensure_scheduled_task(envelope, task)
            }
            pod0_application::ApplicationCommand::UpdateScheduledTask {
                task_id,
                expected_task_revision,
                task,
            } => self.update_scheduled_task(envelope, task_id, expected_task_revision, task),
            pod0_application::ApplicationCommand::RemoveScheduledTask {
                task_id,
                expected_task_revision,
            } => self.remove_scheduled_task(envelope, task_id, expected_task_revision),
            pod0_application::ApplicationCommand::ReconcileScheduledRuns => {
                self.reconcile_scheduled_runs(envelope)
            }
            pod0_application::ApplicationCommand::RetryScheduledRun {
                occurrence_id,
                expected_workflow_revision,
            } => self.retry_scheduled_run(envelope, occurrence_id, expected_workflow_revision),
            pod0_application::ApplicationCommand::CancelScheduledRun {
                occurrence_id,
                expected_workflow_revision,
            } => self.cancel_scheduled_run(envelope, occurrence_id, expected_workflow_revision),
            _ => unreachable!("scheduled-agent dispatcher received another command"),
        }
    }

    pub(super) fn ensure_scheduled_task(
        &mut self,
        envelope: &CommandEnvelope,
        task: ScheduledTaskInput,
    ) {
        let Some(store) = self.scheduled_agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let now = self.now();
        let task_id = task
            .task_id
            .unwrap_or_else(|| ScheduledTaskId::from_bytes(envelope.command_id.into_bytes()));
        let Some(prompt_revision) = scheduled_prompt_revision(&task.prompt) else {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        };
        let definition = ScheduledTaskDefinition {
            task_id,
            label: task.label,
            prompt: task.prompt,
            prompt_revision,
            model_reference: task.model_reference,
            interval_milliseconds: task.interval_milliseconds,
            created_at: now,
            last_run_at: None,
            next_run_at: task.next_run_at,
            revision: StateRevision::new(1),
        };
        let result = store.ensure_task(self.scheduled_context(envelope), definition);
        self.finish_scheduled_agent_command(envelope.command_id, result.map(|_| ()));
    }

    pub(super) fn update_scheduled_task(
        &mut self,
        envelope: &CommandEnvelope,
        task_id: ScheduledTaskId,
        expected_revision: StateRevision,
        task: ScheduledTaskInput,
    ) {
        let Some(store) = self.scheduled_agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        if task.task_id.is_some_and(|value| value != task_id) {
            self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
            return;
        }
        let result = store.task(task_id).and_then(|existing| {
            let existing = existing.ok_or(StorageError::ScheduledAgentTaskNotFound)?;
            let prompt_revision = scheduled_prompt_revision(&task.prompt)
                .ok_or(StorageError::ScheduledAgentWorkflowConflict)?;
            let definition = ScheduledTaskDefinition {
                task_id,
                label: task.label,
                prompt: task.prompt,
                prompt_revision,
                model_reference: task.model_reference,
                interval_milliseconds: task.interval_milliseconds,
                created_at: existing.created_at,
                last_run_at: existing.last_run_at,
                next_run_at: task.next_run_at,
                revision: StateRevision::new(expected_revision.value.saturating_add(1)),
            };
            store
                .update_task(
                    self.scheduled_context(envelope),
                    expected_revision,
                    definition,
                )
                .map(|_| ())
        });
        self.finish_scheduled_agent_command(envelope.command_id, result);
    }

    pub(super) fn remove_scheduled_task(
        &mut self,
        envelope: &CommandEnvelope,
        task_id: ScheduledTaskId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.scheduled_agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let request_ids = self.scheduled_request_ids_for_task(&store, task_id);
        let result = store
            .remove_task(self.scheduled_context(envelope), task_id, expected_revision)
            .map(|_| ());
        if result.is_ok() {
            for request_id in request_ids {
                self.withdraw_scheduled_agent_request(request_id);
            }
        }
        self.finish_scheduled_agent_command(envelope.command_id, result);
    }

    pub(super) fn reconcile_scheduled_runs(&mut self, envelope: &CommandEnvelope) {
        let Some(store) = self.scheduled_agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let result = store.reconcile_due_runs(self.scheduled_context(envelope));
        match result {
            Ok(ScheduledAgentReconcileOutcome { requests, .. }) => {
                for request in requests {
                    let _ = self.queue_scheduled_agent_request(request);
                }
                self.succeed(envelope.command_id, None);
            }
            Err(error) => self.fail(envelope.command_id, storage_failure(error)),
        }
    }

    pub(super) fn cancel_scheduled_run(
        &mut self,
        envelope: &CommandEnvelope,
        occurrence_id: ScheduledOccurrenceId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.scheduled_agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let request_id = store
            .occurrence(occurrence_id)
            .ok()
            .flatten()
            .and_then(|occurrence| occurrence.request_id);
        let result = store
            .cancel_occurrence(
                self.scheduled_context(envelope),
                occurrence_id,
                expected_revision,
            )
            .map(|_| ());
        if result.is_ok()
            && let Some(request_id) = request_id
        {
            self.withdraw_scheduled_agent_request(request_id);
        }
        self.finish_scheduled_agent_command(envelope.command_id, result);
    }

    pub(super) fn retry_scheduled_run(
        &mut self,
        envelope: &CommandEnvelope,
        occurrence_id: ScheduledOccurrenceId,
        expected_revision: StateRevision,
    ) {
        let Some(store) = self.scheduled_agent_store.clone() else {
            self.fail(envelope.command_id, CoreFailureCode::StorageUnavailable);
            return;
        };
        let result = store
            .retry_occurrence(
                self.scheduled_context(envelope),
                occurrence_id,
                expected_revision,
            )
            .map(|_| ());
        self.finish_scheduled_agent_command(envelope.command_id, result);
    }

    fn scheduled_context(&self, envelope: &CommandEnvelope) -> ScheduledAgentCommandContext {
        ScheduledAgentCommandContext {
            command_id: envelope.command_id,
            command_fingerprint: scheduled_agent_command_fingerprint(&envelope.command),
            cancellation_id: envelope.cancellation_id,
            issued_revision: self.revision,
            observed_at: self.now(),
        }
    }

    fn finish_scheduled_agent_command(
        &mut self,
        command_id: CommandId,
        result: Result<(), StorageError>,
    ) {
        match result {
            Ok(()) => self.succeed(command_id, None),
            Err(error) => self.fail(command_id, storage_failure(error)),
        }
    }

    fn scheduled_request_ids_for_task(
        &self,
        store: &ScheduledAgentStore,
        task_id: ScheduledTaskId,
    ) -> Vec<pod0_domain::HostRequestId> {
        self.pending_scheduled_agents
            .values()
            .filter_map(|request| {
                store
                    .occurrence(request.execution.occurrence_id)
                    .ok()
                    .flatten()
                    .filter(|occurrence| occurrence.task_id == task_id)
                    .map(|_| request.request_id)
            })
            .collect()
    }
}
