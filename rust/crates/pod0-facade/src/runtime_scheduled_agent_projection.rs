use pod0_application::{
    ScheduledAgentProjection, ScheduledTaskDefinition, ScheduledTaskProjection,
};
use pod0_domain::ScheduledTaskId;

use crate::runtime_failure::failure;
use crate::runtime_state::FacadeState;
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(crate) fn scheduled_agent_projection(
        &self,
        task_id: Option<ScheduledTaskId>,
        offset: u32,
        max_items: u16,
    ) -> ScheduledAgentProjection {
        let Some(store) = self.scheduled_agent_store.as_ref() else {
            return unavailable_projection();
        };
        let tasks = match task_id {
            Some(task_id) => store
                .task(task_id)
                .map(|task| (task.into_iter().collect::<Vec<_>>(), false)),
            None => store
                .task_page(offset, max_items)
                .map(|page| (page.items, page.has_more)),
        };
        let occurrences = store.occurrence_page(task_id, offset, max_items);
        match (tasks, occurrences) {
            (Ok((tasks, task_has_more)), Ok(occurrences)) => ScheduledAgentProjection {
                tasks: tasks.into_iter().map(task_projection).collect(),
                workflows: occurrences
                    .items
                    .into_iter()
                    .map(|state| state.projection())
                    .collect(),
                has_more: task_has_more || occurrences.has_more,
                failure: None,
            },
            (Err(error), _) | (_, Err(error)) => ScheduledAgentProjection {
                tasks: Vec::new(),
                workflows: Vec::new(),
                has_more: false,
                failure: Some(failure(storage_failure(error))),
            },
        }
    }
}

fn task_projection(task: ScheduledTaskDefinition) -> ScheduledTaskProjection {
    ScheduledTaskProjection {
        task_id: task.task_id,
        label: task.label,
        prompt: task.prompt,
        prompt_revision: task.prompt_revision,
        model_reference: task.model_reference,
        interval_milliseconds: task.interval_milliseconds,
        last_run_at: task.last_run_at,
        next_run_at: task.next_run_at,
        task_revision: task.revision,
    }
}

fn unavailable_projection() -> ScheduledAgentProjection {
    ScheduledAgentProjection {
        tasks: Vec::new(),
        workflows: Vec::new(),
        has_more: false,
        failure: Some(failure(
            pod0_application::CoreFailureCode::StorageUnavailable,
        )),
    }
}
