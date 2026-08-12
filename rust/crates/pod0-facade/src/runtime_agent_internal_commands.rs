use pod0_application::{AgentExecutionKind, AgentTurnStage, InternalCommandKind, agent_tool_policy};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(crate) fn resume_agent_internal_commands(&mut self) {
        let (Some(store), Some(agent_store)) = (self.store.clone(), self.agent_store.clone())
        else {
            return;
        };
        for _ in 0..100 {
            let Ok(commands) = store.pending_internal_commands(100) else {
                return;
            };
            let mut progressed = false;
            for command in commands {
                progressed |= match &command.request.kind {
                    InternalCommandKind::AdvanceAgentTurn { .. } => {
                        self.execute_agent_internal_command(&agent_store, command)
                    }
                    InternalCommandKind::ExecuteAgentProjection { .. } => {
                        self.execute_agent_projection_command(&agent_store, command)
                    }
                    InternalCommandKind::ExecuteAgentTool { .. } => {
                        self.execute_agent_tool_command(&agent_store, command)
                    }
                    InternalCommandKind::CompleteAgentTool { .. } => {
                        self.complete_agent_tool_command(&agent_store, command)
                    }
                    _ => false,
                };
            }
            if !progressed {
                break;
            }
        }
    }

    fn execute_agent_internal_command(
        &mut self,
        store: &pod0_storage::AgentStore,
        command: pod0_storage::PendingInternalCommand,
    ) -> bool {
        let Ok(state) = store.begin_execution_from_internal_command(command, self.now()) else {
            return false;
        };
        if state.projection().stage != AgentTurnStage::Executing {
            return false;
        }
        let Some(execution) = state
            .projection()
            .proposal
            .as_ref()
            .map(|proposal| agent_tool_policy(proposal.action.tool()).execution)
        else {
            return false;
        };
        match execution {
            AgentExecutionKind::RustCommit | AgentExecutionKind::RustProjection => {
                let durable_handoff = matches!(
                    state
                        .projection()
                        .proposal
                        .as_ref()
                        .map(|value| &value.action),
                    Some(
                        pod0_application::AgentToolAction::CreateNote { .. }
                            | pod0_application::AgentToolAction::RecordMemory { .. }
                            | pod0_application::AgentToolAction::CreateClip { .. }
                            | pod0_application::AgentToolAction::WriteCategory { .. }
                            | pod0_application::AgentToolAction::TagItems { .. }
                    )
                );
                let recall_workflow = matches!(
                    state
                        .projection()
                        .proposal
                        .as_ref()
                        .map(|value| &value.action),
                    Some(pod0_application::AgentToolAction::QueryTranscripts { .. })
                );
                if (execution == AgentExecutionKind::RustCommit && !durable_handoff)
                    || recall_workflow
                {
                    let _ = self.execute_internal_agent_action(store, state, self.now());
                }
            }
            AgentExecutionKind::NativeCapability
            | AgentExecutionKind::NativeConversationPresentation
            | AgentExecutionKind::NativeCapabilityAndNmpPublication => {}
        }
        true
    }

    fn execute_agent_projection_command(
        &mut self,
        store: &pod0_storage::AgentStore,
        command: pod0_storage::PendingInternalCommand,
    ) -> bool {
        let InternalCommandKind::ExecuteAgentProjection { turn_id } = &command.request.kind else {
            return false;
        };
        let Ok(Some(state)) = store.turn(*turn_id) else {
            return false;
        };
        let Some(proposal) = state.projection().proposal else {
            return false;
        };
        if agent_tool_policy(proposal.action.tool()).execution != AgentExecutionKind::RustProjection
        {
            return false;
        }
        let result = self
            .perform_internal_agent_action(&proposal.action)
            .map_err(str::to_owned);
        if store
            .commit_projection_result(command, result, self.now())
            .is_err()
        {
            return false;
        }
        self.advance_revision();
        true
    }

    fn execute_agent_tool_command(
        &mut self,
        store: &pod0_storage::AgentStore,
        command: pod0_storage::PendingInternalCommand,
    ) -> bool {
        if command.request.target != pod0_application::ActivityDomain::UserArtifact {
            return false;
        }
        let InternalCommandKind::ExecuteAgentTool { turn_id } = &command.request.kind else {
            return false;
        };
        let Ok(Some(state)) = store.turn(*turn_id) else {
            return false;
        };
        let Some(action) = state.projection().proposal.map(|value| value.action) else {
            return false;
        };
        let committed = match action {
            pod0_application::AgentToolAction::CreateNote { .. } => store
                .commit_agent_note(command, self.now())
                .and_then(|_| self.reload_notes()),
            pod0_application::AgentToolAction::RecordMemory { .. } => store
                .commit_agent_memory(command, self.now())
                .and_then(|_| self.reload_memories()),
            pod0_application::AgentToolAction::CreateClip { .. } => store
                .commit_agent_clip(command, self.now())
                .and_then(|_| self.reload_clips()),
            pod0_application::AgentToolAction::WriteCategory { .. }
            | pod0_application::AgentToolAction::TagItems { .. } => store
                .commit_agent_category(command, self.now())
                .map(|_| ()),
            _ => return false,
        };
        if committed.is_err() {
            return false;
        }
        self.advance_revision();
        true
    }

    fn complete_agent_tool_command(
        &mut self,
        store: &pod0_storage::AgentStore,
        command: pod0_storage::PendingInternalCommand,
    ) -> bool {
        if store.commit_tool_completion(command, self.now()).is_err() {
            return false;
        }
        self.advance_revision();
        true
    }
}
