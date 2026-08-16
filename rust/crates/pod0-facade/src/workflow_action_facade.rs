use pod0_application::{
    ApplicationCommand, CommandEnvelope, DownloadIntentOrigin, TranscriptProvider,
    TranscriptWorkflowConfiguration, WorkflowActionDispatchResult, WorkflowActionKind,
    WorkflowActionTarget, WorkflowActionToken,
};
use pod0_domain::{CancellationId, CommandId, ContentDigest};

use crate::Pod0Facade;
use crate::runtime_state::FacadeState;

#[uniffi::export]
impl Pod0Facade {
    /// Executes only an exact action token emitted by the current Rust
    /// workflow projection. Native code cannot select configuration, target,
    /// or expected revision independently of that projection.
    pub fn execute_workflow_action(
        &self,
        token: WorkflowActionToken,
    ) -> WorkflowActionDispatchResult {
        if !token.is_structurally_valid() {
            return WorkflowActionDispatchResult::InvalidToken;
        }
        let mut state = self.state();
        let command = match command_for_workflow_action(&state, token) {
            Ok(command) => command,
            Err(result) => return result,
        };
        let command_id = CommandId::from_bytes(
            token.authorization.into_bytes()[..16]
                .try_into()
                .expect("content digest prefix"),
        );
        let cancellation_id = cancellation_id(token.authorization);
        let changed = state.dispatch(CommandEnvelope {
            command_id,
            cancellation_id,
            // The exact workflow revision is embedded in the typed command.
            // The envelope revision fences the unrelated global facade
            // projection and must not be overloaded for workflow actions.
            expected_revision: None,
            command,
        });
        let Some(operation) = state
            .operations
            .iter()
            .find(|value| value.command_id == command_id)
        else {
            return WorkflowActionDispatchResult::StorageUnavailable;
        };
        let result = match operation.stage {
            pod0_application::OperationStage::Failed => {
                match operation.failure.as_ref().map(|v| v.code) {
                    Some(pod0_application::CoreFailureCode::NotFound) => {
                        WorkflowActionDispatchResult::NotFound
                    }
                    Some(pod0_application::CoreFailureCode::RevisionConflict) => {
                        WorkflowActionDispatchResult::Stale
                    }
                    Some(pod0_application::CoreFailureCode::InvalidCommand) => {
                        WorkflowActionDispatchResult::NotAllowed
                    }
                    _ => WorkflowActionDispatchResult::StorageUnavailable,
                }
            }
            pod0_application::OperationStage::Unsupported { .. } => {
                WorkflowActionDispatchResult::NotAllowed
            }
            _ => WorkflowActionDispatchResult::Accepted,
        };
        drop(state);
        if changed {
            self.notify_subscribers();
        }
        result
    }
}

fn command_for_workflow_action(
    state: &FacadeState,
    token: WorkflowActionToken,
) -> Result<ApplicationCommand, WorkflowActionDispatchResult> {
    let store = state
        .store
        .as_ref()
        .ok_or(WorkflowActionDispatchResult::StorageUnavailable)?;
    let revision = token.expected_workflow_revision;
    match token.target {
        WorkflowActionTarget::PublisherChapters { episode_id } => {
            let current = store
                .publisher_chapter_workflow(episode_id)
                .map_err(|_| WorkflowActionDispatchResult::StorageUnavailable)?
                .ok_or(WorkflowActionDispatchResult::NotFound)?;
            if current.workflow_revision != revision {
                return Err(WorkflowActionDispatchResult::Stale);
            }
            Ok(match token.action {
                WorkflowActionKind::Retry => ApplicationCommand::RetryPublisherChapters {
                    episode_id,
                    expected_workflow_revision: revision,
                },
                WorkflowActionKind::Cancel => ApplicationCommand::CancelPublisherChapters {
                    episode_id,
                    expected_workflow_revision: revision,
                },
            })
        }
        WorkflowActionTarget::ModelChapters { episode_id } => {
            let current = store
                .model_chapter_workflow(episode_id)
                .map_err(|_| WorkflowActionDispatchResult::StorageUnavailable)?
                .ok_or(WorkflowActionDispatchResult::NotFound)?;
            if current.workflow_revision != revision {
                return Err(WorkflowActionDispatchResult::Stale);
            }
            Ok(match token.action {
                WorkflowActionKind::Retry => ApplicationCommand::RetryModelChapters {
                    episode_id,
                    configured_model: current.desired_configured_model,
                    expected_workflow_revision: revision,
                },
                WorkflowActionKind::Cancel => ApplicationCommand::CancelModelChapters {
                    episode_id,
                    expected_workflow_revision: revision,
                },
            })
        }
        WorkflowActionTarget::Download { episode_id } => {
            let current = store
                .download_workflow(episode_id)
                .map_err(|_| WorkflowActionDispatchResult::StorageUnavailable)?
                .ok_or(WorkflowActionDispatchResult::NotFound)?;
            if current.workflow_revision != revision {
                return Err(WorkflowActionDispatchResult::Stale);
            }
            Ok(match token.action {
                WorkflowActionKind::Retry => ApplicationCommand::RequestEpisodeDownload {
                    episode_id,
                    origin: match current.origin {
                        pod0_storage::StoredDownloadOrigin::User => DownloadIntentOrigin::User,
                        pod0_storage::StoredDownloadOrigin::Playback => {
                            DownloadIntentOrigin::Playback
                        }
                        pod0_storage::StoredDownloadOrigin::Automatic => {
                            DownloadIntentOrigin::Automatic
                        }
                        pod0_storage::StoredDownloadOrigin::Unsupported(wire_code) => {
                            DownloadIntentOrigin::Unsupported { wire_code }
                        }
                    },
                },
                WorkflowActionKind::Cancel => ApplicationCommand::CancelEpisodeDownload {
                    episode_id,
                    expected_workflow_revision: revision,
                },
            })
        }
        WorkflowActionTarget::Transcript { episode_id } => {
            let current = store
                .transcript_workflow(episode_id)
                .map_err(|_| WorkflowActionDispatchResult::StorageUnavailable)?
                .ok_or(WorkflowActionDispatchResult::NotFound)?;
            if current.workflow_revision != revision {
                return Err(WorkflowActionDispatchResult::Stale);
            }
            let provider = transcript_provider(&current.request.provider);
            let capabilities = store
                .workflow_capability_snapshot()
                .map_err(|_| WorkflowActionDispatchResult::StorageUnavailable)?
                .ok_or(WorkflowActionDispatchResult::NotAllowed)?;
            Ok(match token.action {
                WorkflowActionKind::Cancel => ApplicationCommand::CancelTranscriptWorkflow {
                    episode_id,
                    expected_workflow_revision: revision,
                },
                WorkflowActionKind::Retry => ApplicationCommand::RetryTranscriptWorkflow {
                    episode_id,
                    expected_workflow_revision: revision,
                    configuration: TranscriptWorkflowConfiguration {
                        provider,
                        model: current.request.model,
                        local_audio_url: current.request.local_audio_url,
                        credential_available: capabilities.credentials.available(provider),
                        auto_publisher_enabled: current.request.publisher_first,
                        auto_provider_enabled: current.request.provider_fallback_enabled,
                    },
                },
            })
        }
        WorkflowActionTarget::ScheduledAgent { .. } => {
            Err(WorkflowActionDispatchResult::NotAllowed)
        }
    }
}

fn transcript_provider(value: &str) -> TranscriptProvider {
    match value {
        "assembly-ai" => TranscriptProvider::AssemblyAi,
        "elevenlabs-scribe" => TranscriptProvider::ElevenLabsScribe,
        "openrouter-whisper" => TranscriptProvider::OpenRouterWhisper,
        "apple-speech" => TranscriptProvider::AppleSpeech,
        _ => TranscriptProvider::Unsupported { wire_code: 1 },
    }
}

fn cancellation_id(digest: ContentDigest) -> CancellationId {
    let mut bytes = digest.into_bytes();
    bytes[..16].reverse();
    CancellationId::from_bytes(bytes[..16].try_into().expect("content digest prefix"))
}
