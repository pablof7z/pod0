use pod0_application::{
    ActivitySubject, AgentApprovalRequest, AgentCapabilityExecutionMode, AgentCapabilityRequest,
    AgentGeneratedAudioTarget, AgentModelExecutionRequest, AgentToolAction, AgentTurnStage,
    ExternalEffectKind, HostRequest, HostRequestEnvelope, LeasedHostRequestEnvelope,
    MAX_AGENT_GENERATED_AUDIO_BYTES, MAX_AGENT_MODEL_OUTPUT_BYTES, RecallEmbeddingInput,
    agent_generated_artifact_id, agent_tool_definitions, bounded_host_request_count,
};
use pod0_domain::{CancellationId, CommandId, HostRequestId};
use pod0_recall_index::{RECALL_INDEX_DIMENSIONS, RecallIndexPlan};
use sha2::{Digest as _, Sha256};

use crate::runtime_evidence_commands::index_spans;
use crate::runtime_evidence_state::{EvidenceIndexCompletion, PendingEvidenceIndex};
use crate::runtime_state::FacadeState;
use crate::runtime_transcript_workflow_mapping::host_request;

const EFFECT_LEASE_MILLISECONDS: u32 = 120_000;

impl FacadeState {
    pub(super) fn next_leased_transcript_requests(
        &mut self,
        maximum_count: u16,
    ) -> (bool, Vec<LeasedHostRequestEnvelope>) {
        let maximum = bounded_host_request_count(maximum_count);
        let mut changed = false;
        let mut requests = Vec::with_capacity(maximum);
        while requests.len() < maximum {
            changed |= self.prepare_transcript_host_request();
            let Some(store) = self.store.clone() else {
                break;
            };
            let Ok(Some(lease)) = store.claim_next_effect(self.now(), EFFECT_LEASE_MILLISECONDS)
            else {
                break;
            };
            let Some(request) = self.host_request_for_effect(&lease) else {
                break;
            };
            if !self.host_requests.register(request.clone())
                && !self.host_requests.matches_outstanding(&request)
            {
                break;
            }
            requests.push(LeasedHostRequestEnvelope {
                lease: lease.identity(),
                request,
            });
        }
        (changed, requests)
    }

    fn host_request_for_effect(
        &mut self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        match lease.request.kind {
            ExternalEffectKind::TranscriptProvider => self.transcript_request_for_effect(lease),
            ExternalEffectKind::RecallProvider => self.recall_request_for_effect(lease),
            ExternalEffectKind::AgentProvider => self.agent_model_request_for_effect(lease),
            ExternalEffectKind::AgentApproval => self.agent_approval_request_for_effect(lease),
            ExternalEffectKind::AgentCapability => self.agent_capability_request_for_effect(lease),
            _ => None,
        }
    }

    fn agent_capability_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let ActivitySubject::AgentTurn { turn_id } = lease.subject else {
            return None;
        };
        let state = self.agent_store.as_ref()?.turn(turn_id).ok()??;
        let projection = state.projection();
        if projection.stage != AgentTurnStage::Executing {
            return None;
        }
        let proposal = projection.proposal?;
        let execution_fence_id = projection.execution_fence_id?;
        let generated_audio_target =
            matches!(proposal.action, AgentToolAction::GenerateTtsEpisode { .. }).then(|| {
                AgentGeneratedAudioTarget {
                    artifact_id: agent_generated_artifact_id(
                        proposal.proposal_id,
                        proposal.proposal_digest,
                    ),
                    maximum_bytes: MAX_AGENT_GENERATED_AUDIO_BYTES,
                }
            });
        Some(HostRequestEnvelope {
            request_id: pod0_application::agent_capability_request_id(
                turn_id,
                proposal.proposal_id,
                execution_fence_id,
            ),
            command_id: CommandId::from_bytes(lease.intent_id.into_bytes()),
            cancellation_id: state.cancellation_id(),
            issued_revision: self.revision,
            deadline_at: lease.request.deadline_at,
            request: HostRequest::ExecuteAgentCapability {
                capability: AgentCapabilityRequest {
                    turn_id,
                    proposal_id: proposal.proposal_id,
                    proposal_digest: proposal.proposal_digest,
                    execution_fence_id,
                    execution_mode: if lease.fence == 1 {
                        AgentCapabilityExecutionMode::Perform
                    } else {
                        AgentCapabilityExecutionMode::RecoverExisting
                    },
                    generated_audio_target,
                    action: proposal.action,
                },
            },
        })
    }

    fn agent_approval_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let ActivitySubject::AgentTurn { turn_id } = lease.subject else {
            return None;
        };
        let state = self.agent_store.as_ref()?.turn(turn_id).ok()??;
        let projection = state.projection();
        if projection.stage != AgentTurnStage::ApprovalRequired {
            return None;
        }
        let proposal = projection.proposal?;
        Some(HostRequestEnvelope {
            request_id: pod0_application::agent_approval_request_id(
                turn_id,
                proposal.proposal_id,
                proposal.proposal_digest,
            ),
            command_id: CommandId::from_bytes(lease.intent_id.into_bytes()),
            cancellation_id: state.cancellation_id(),
            issued_revision: self.revision,
            deadline_at: lease.request.deadline_at,
            request: HostRequest::PresentAgentApproval {
                approval: AgentApprovalRequest { turn_id, proposal },
            },
        })
    }

    fn agent_model_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let ActivitySubject::AgentTurn { turn_id } = lease.subject else {
            return None;
        };
        let state = self.agent_store.as_ref()?.turn(turn_id).ok()??;
        let projection = state.projection();
        if projection.stage != AgentTurnStage::AwaitingModel {
            return None;
        }
        let model_fence_id = projection.execution_fence_id?;
        let available_tools = if projection.commit.is_some() {
            Vec::new()
        } else {
            state.available_tools().to_vec()
        };
        let tool_definitions = agent_tool_definitions(&available_tools)?;
        Some(HostRequestEnvelope {
            request_id: crate::runtime_agent_modules::identity::model_request_id(
                turn_id,
                model_fence_id,
            ),
            command_id: CommandId::from_bytes(lease.intent_id.into_bytes()),
            cancellation_id: state.cancellation_id(),
            issued_revision: self.revision,
            deadline_at: lease.request.deadline_at,
            request: HostRequest::ExecuteAgentModelTurn {
                execution: AgentModelExecutionRequest {
                    conversation_id: projection.conversation_id,
                    turn_id,
                    model_fence_id,
                    model_reference: state.model_reference().to_owned(),
                    messages: self.agent_model_messages(&projection),
                    tool_definitions,
                    maximum_output_bytes: MAX_AGENT_MODEL_OUTPUT_BYTES,
                },
            },
        })
    }

    fn transcript_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let ActivitySubject::TranscriptWorkflow { workflow_id } = lease.subject else {
            return None;
        };
        let episode_id = lease.episode_id?;
        let store = self.store.as_ref()?;
        let record = store.transcript_workflow(episode_id).ok()??;
        if record.request.workflow_id != workflow_id {
            return None;
        }
        let podcast_id = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)?
            .podcast_id;
        let request = host_request(&record, podcast_id)?;
        (lease.request.deadline_at == request.deadline_at).then_some(request)
    }

    fn recall_request_for_effect(
        &mut self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let ActivitySubject::Episode { episode_id } = lease.subject else {
            return None;
        };
        let artifact = self
            .evidence_store
            .as_ref()?
            .selected_artifact(episode_id)
            .ok()??;
        let spans = index_spans(&artifact);
        let plan = self.recall_index.prepare_episode(
            &spans,
            self.begin_recall_index_operation(CancellationId::from_bytes(
                lease.intent_id.into_bytes(),
            ))
            .cancellation(),
        );
        let requested = match plan.ok()? {
            RecallIndexPlan::NeedsEmbeddings { spans } => spans,
            RecallIndexPlan::Ready { .. } => Vec::new(),
        };
        let workflow = self
            .store
            .as_ref()?
            .transcript_workflow(episode_id)
            .ok()??;
        let command_id = workflow.command_id;
        let cancellation_id = workflow.cancellation_id;
        let requested_span_ids = requested
            .iter()
            .map(|span| span.span_id)
            .collect::<Vec<_>>();
        let request_id = recall_request_id(lease.intent_id, &requested_span_ids);
        self.pending_evidence_indexes.insert(
            request_id,
            PendingEvidenceIndex {
                command_id,
                cancellation_id,
                episode_id,
                generation_id: artifact.generation_id,
                expected_span_count: u32::try_from(artifact.spans.len()).ok()?,
                requested_span_ids,
                completion: EvidenceIndexCompletion::TranscriptWorkflow {
                    workflow_id: workflow.request.workflow_id,
                    input_version: workflow.evidence_input_version?,
                },
            },
        );
        Some(HostRequestEnvelope {
            request_id,
            command_id,
            cancellation_id,
            issued_revision: self.revision,
            deadline_at: lease.request.deadline_at,
            request: HostRequest::EmbedRecallSpans {
                episode_id,
                generation_id: artifact.generation_id,
                provider: self.recall_configuration.embedding_provider,
                model: self.recall_configuration.embedding_model.clone(),
                spans: requested
                    .into_iter()
                    .map(|span| RecallEmbeddingInput {
                        span_id: span.span_id,
                        text: span.text,
                    })
                    .collect(),
                maximum_dimensions: u16::try_from(RECALL_INDEX_DIMENSIONS).ok()?,
            },
        })
    }
}

fn recall_request_id(
    intent_id: pod0_domain::EffectIntentId,
    spans: &[pod0_domain::EvidenceSpanId],
) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0/evidence/leased-request/v1");
    hash.update(intent_id.into_bytes());
    for span in spans {
        hash.update(span.into_bytes());
    }
    let digest: [u8; 32] = hash.finalize().into();
    HostRequestId::from_bytes(digest[..16].try_into().expect("fixed digest prefix"))
}
