use pod0_application::{
    DurableAgentApprovalHostObservation, DurableAgentModelHostObservation,
    DurableRecallHostObservation, DurableTranscriptHostObservation, ExternalEffectKind,
    HostObservationReceipt, HostObservationRejection, LeasedHostObservationEnvelope,
    TranscriptCapabilityValidation, TranscriptObservationPolicyInput,
    TranscriptObservationPolicyState, decide_transcript_observation,
    validate_transcript_capability_observation,
};
use pod0_storage::TranscriptObservationCommitInput;

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::FacadeState;
use crate::runtime_transcript_workflow_receipts::storage_receipt;

impl FacadeState {
    pub(super) fn record_leased_host_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let Some(store) = self.store.as_ref() else {
            return (false, retain(request_id));
        };
        let effect_kind = match store.effect_kind(leased.lease.intent_id) {
            Ok(Some(kind)) => kind,
            Ok(None) => {
                return (
                    false,
                    rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
                );
            }
            Err(_) => return (false, retain(request_id)),
        };
        match effect_kind {
            ExternalEffectKind::AgentCapability => {
                let Some(agent) =
                    pod0_application::DurableAgentCapabilityHostObservation::from_host(
                        &leased.observation,
                    )
                else {
                    return mismatched(request_id);
                };
                return self.record_leased_agent_capability_observation(leased, agent);
            }
            ExternalEffectKind::AgentApproval => {
                let Some(agent) =
                    DurableAgentApprovalHostObservation::from_host(&leased.observation)
                else {
                    return mismatched(request_id);
                };
                return self.record_leased_agent_approval_observation(leased, agent);
            }
            ExternalEffectKind::AgentProvider => {
                let Some(agent) = DurableAgentModelHostObservation::from_host(&leased.observation)
                else {
                    return mismatched(request_id);
                };
                return self.record_leased_agent_model_observation(leased, agent);
            }
            ExternalEffectKind::RecallProvider => {
                let execution = store.effect_request(leased.lease.intent_id).ok().flatten();
                if execution.as_ref().is_some_and(|request| {
                    matches!(
                        request.execution,
                        pod0_application::DurableEffectExecution::RecallIndexCutover { .. }
                    )
                }) {
                    let Some(cutover) =
                        pod0_application::DurableRecallIndexCutoverHostObservation::from_host(
                            &leased.observation,
                        )
                    else {
                        return mismatched(request_id);
                    };
                    return self.record_leased_recall_index_cutover_observation(leased, cutover);
                }
                if execution.as_ref().is_some_and(|request| {
                    matches!(
                        request.execution,
                        pod0_application::DurableEffectExecution::RecallQuery { .. }
                    )
                }) {
                    let Some(recall) =
                        pod0_application::DurableAgentRecallHostObservation::from_host(
                            &leased.observation,
                        )
                    else {
                        return mismatched(request_id);
                    };
                    return self.record_leased_query_recall_observation(leased, recall);
                }
                let is_agent_recall = execution.is_some_and(|request| {
                    matches!(
                        request.execution,
                        pod0_application::DurableEffectExecution::AgentRecall { .. }
                    )
                });
                if is_agent_recall {
                    let Some(recall) =
                        pod0_application::DurableAgentRecallHostObservation::from_host(
                            &leased.observation,
                        )
                    else {
                        return mismatched(request_id);
                    };
                    return self.record_leased_agent_recall_observation(leased, recall);
                }
                let Some(recall) = DurableRecallHostObservation::from_host(&leased.observation)
                else {
                    return mismatched(request_id);
                };
                return self.record_leased_recall_observation(leased, recall);
            }
            ExternalEffectKind::PublisherChapterProvider => {
                return self.record_leased_publisher_chapter_observation(leased);
            }
            ExternalEffectKind::ModelChapterProvider => {
                return self.record_leased_model_chapter_observation(leased);
            }
            ExternalEffectKind::Download => {
                return self.record_leased_download_observation(leased);
            }
            ExternalEffectKind::Playback => {
                return self.record_leased_playback_observation(leased);
            }
            ExternalEffectKind::FeedNetwork => {
                return self.record_leased_feed_observation(leased);
            }
            ExternalEffectKind::Notification => {
                return self.record_leased_feed_notification_observation(leased);
            }
            ExternalEffectKind::ScheduledAgentProvider => {
                return self.record_leased_scheduled_agent_observation(leased);
            }
            ExternalEffectKind::CoreWake => {
                return self.record_leased_lifecycle_wake_observation(leased);
            }
            ExternalEffectKind::Cancellation => {
                return self.record_leased_cancellation_observation(leased);
            }
            ExternalEffectKind::LibraryNetwork => {
                return self.record_leased_library_network_observation(leased);
            }
            ExternalEffectKind::TranscriptProvider => {}
            _ => return mismatched(request_id),
        }
        self.record_leased_transcript_observation(leased)
    }

    fn record_leased_transcript_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let Some(durable) = DurableTranscriptHostObservation::from_host(&leased.observation) else {
            return mismatched(request_id);
        };
        if !matches!(
            validate_transcript_capability_observation(durable.observation.clone()),
            TranscriptCapabilityValidation::Accepted
        ) {
            return mismatched(request_id);
        }
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let Ok(Some(record)) = store.transcript_workflow_for_effect_intent(leased.lease.intent_id)
        else {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::StaleWorkflow),
            );
        };
        let committed_at = self.now();
        let decision = decide_transcript_observation(TranscriptObservationPolicyInput {
            state: TranscriptObservationPolicyState {
                workflow_id: record.request.workflow_id,
                workflow_revision: record.workflow_revision,
                attempt: record.attempt,
                max_attempts: record.max_attempts,
                submission_authorized: record.submission_authorized_at_ms.is_some(),
                provider_accepted: record.external_operation_id.is_some(),
            },
            observation: durable.observation.clone(),
            observed_at: committed_at,
            retry_issued_revision: self.revision,
        });
        let outcome = store.commit_transcript_observation(TranscriptObservationCommitInput {
            lease: leased.lease,
            observation: durable,
            decision,
            committed_at,
        });
        let committed = match outcome {
            Ok(value) => value,
            Err(error) => return (false, storage_receipt(request_id, error)),
        };
        self.retire_transcript_request(request_id);
        if committed.replayed {
            return (
                false,
                rejected_payload(request_id, HostObservationRejection::Duplicate),
            );
        }
        self.advance_revision();
        self.apply_committed_transcript_observation(&committed.workflow);
        (true, persisted(request_id, true))
    }
}

fn mismatched(request_id: pod0_domain::HostRequestId) -> (bool, HostObservationReceipt) {
    (
        false,
        rejected_payload(request_id, HostObservationRejection::MismatchedPayload),
    )
}
