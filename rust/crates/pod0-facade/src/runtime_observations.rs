use pod0_application::{
    CoreFailureCode, HostFailureCode, HostObservation, HostObservationEnvelope,
    HostObservationReceipt, HostObservationRejection, ObservationAcceptance, OperationStage,
};

use crate::runtime_observation_mapping::{accepted, host_failure, rejected, retain};
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn record_host_observation(
        &mut self,
        observation: HostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = observation.request_id;
        if let Some(result) = self.retry_pending_durable_observation(&observation) {
            return result;
        }
        if let Some(result) =
            self.retry_pending_scheduled_agent_observation(request_id, &observation)
        {
            return result;
        }
        if let Some(result) = self.retry_pending_agent_observation(request_id, &observation) {
            return result;
        }
        if let Some(result) = self.retry_pending_agent_recall_observation(&observation) {
            return result;
        }
        if let Some(receipt) = self.replayed_model_completion_receipt(&observation) {
            return (false, receipt);
        }
        if let Some(record) = self.late_ambiguous_model_record(&observation) {
            let receipt = self.persist_model_observation(record, observation);
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            return (changed, receipt);
        }
        if let Some(pending) = self.pending_model_observations.get(&request_id).cloned() {
            if pending != observation {
                return (false, retain(request_id));
            }
            let record = self
                .pending_model_chapters
                .get(&request_id)
                .and_then(|episode_id| {
                    self.store
                        .as_ref()
                        .and_then(|store| store.model_chapter_workflow(*episode_id).ok())
                        .flatten()
                });
            let Some(record) = record else {
                return (false, retain(request_id));
            };
            let receipt = self.persist_model_observation(record, observation);
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            if !matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_model_observations.remove(&request_id);
            }
            return (changed, receipt);
        }
        if let Some(result) = self.retry_pending_transcript_observation(request_id, &observation) {
            return result;
        }
        if self
            .pending_publisher_observations
            .contains_key(&request_id)
        {
            return (false, accepted(request_id));
        }
        let command_id = self.host_requests.command_id(observation.request_id);
        let is_playback_request = self
            .host_requests
            .is_playback_request(observation.request_id);
        let pending_recall = self.pending_recalls.get(&observation.request_id).copied();
        let pending_evidence = self
            .pending_evidence_indexes
            .get(&observation.request_id)
            .cloned();
        let pending_cutover = self
            .pending_recall_cutovers
            .get(&observation.request_id)
            .copied();
        let pending_publisher = self
            .pending_publisher_chapters
            .get(&observation.request_id)
            .cloned();
        let pending_model = self
            .pending_model_chapters
            .get(&request_id)
            .and_then(|episode_id| {
                self.store
                    .as_ref()
                    .and_then(|store| store.model_chapter_workflow(*episode_id).ok())
                    .flatten()
            });
        let pending_transcript = self.pending_transcript_record(request_id);
        let pending_wake = self.pending_core_wakes.contains_key(&request_id);
        let pending_download = self.pending_downloads.contains_key(&request_id);
        let pending_feed_notification = self
            .pending_feed_discovery_notifications
            .contains_key(&request_id);
        let pending_scheduled_agent = self.pending_scheduled_agents.contains_key(&request_id);
        let pending_agent = self.pending_agents.contains_key(&request_id);
        let acceptance = self.host_requests.accept_observation(&observation);
        if acceptance == ObservationAcceptance::PayloadTooLarge
            && let Some(record) = pending_model
        {
            let receipt = self.persist_oversized_model_observation(record);
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            return (changed, receipt);
        }
        if acceptance == ObservationAcceptance::PayloadTooLarge && pending_publisher.is_some() {
            self.advance_revision();
            self.pending_publisher_observations.insert(
                observation.request_id,
                HostObservation::Failed {
                    code: HostFailureCode::ResponseTooLarge,
                    safe_detail: None,
                },
            );
            self.retry_pending_publisher_observations();
            self.trim_operations();
            return (true, accepted(request_id));
        }
        if acceptance != ObservationAcceptance::Accepted {
            return (false, rejected(request_id, acceptance));
        }
        let Some(command_id) = command_id else {
            return (
                false,
                HostObservationReceipt::Rejected {
                    request_id,
                    reason: HostObservationRejection::UnknownRequest,
                },
            );
        };
        if pending_agent {
            return self.accept_agent_observation(observation);
        }
        if pending_scheduled_agent {
            let retained = observation.clone();
            let receipt = self.persist_scheduled_agent_observation(observation);
            if matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_scheduled_agent_observations
                    .insert(request_id, retained);
            }
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            return (changed, receipt);
        }
        if let Some(record) = pending_model {
            let retained = observation.clone();
            let receipt = self.persist_model_observation(record, observation);
            if matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_model_observations.insert(request_id, retained);
            }
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            return (changed, receipt);
        }
        if let Some(record) = pending_transcript {
            let retained = observation.clone();
            let receipt = self.persist_transcript_observation(record, observation);
            if matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_transcript_observations
                    .insert(request_id, retained);
            }
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            return (changed, receipt);
        }
        if pending_feed_notification {
            let retained = observation.clone();
            let result = self.persist_feed_discovery_notification_observation(observation);
            if matches!(result.1, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_feed_discovery_notification_observations
                    .insert(request_id, retained);
            }
            return result;
        }
        if pending_wake {
            let changed = self.finish_core_wake(request_id, observation.observation);
            return (changed, accepted(request_id));
        }
        if pending_download {
            let retained = observation.clone();
            let receipt = self.persist_download_observation(observation);
            if matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_download_observations
                    .insert(request_id, retained);
            }
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            return (changed, receipt);
        }
        self.advance_revision();
        if pending_publisher.is_some() {
            self.pending_publisher_observations
                .insert(observation.request_id, observation.observation);
            self.retry_pending_publisher_observations();
        } else if let Some(pending) = self.pending_feeds.remove(&observation.request_id) {
            self.finish_feed_observation(
                pending,
                observation.observation,
                observation.observed_at.value,
            );
        } else if let Some(pending) = pending_recall {
            if self.finish_recall_observation_for_agent(request_id, pending, observation) {
                return (false, retain(request_id));
            }
        } else if let Some(pending) = pending_evidence {
            self.pending_evidence_indexes
                .remove(&observation.request_id);
            self.finish_evidence_index_observation(pending, observation.observation);
        } else if let Some(pending) = pending_cutover {
            self.pending_recall_cutovers.remove(&observation.request_id);
            self.finish_recall_index_cutover(pending, observation.observation);
        } else {
            match observation.observation {
                HostObservation::Failed { code, .. } if is_playback_request => {
                    let _ = code;
                    self.playback_host_failed(command_id);
                }
                HostObservation::Failed { code, .. } => self.fail(command_id, host_failure(code)),
                HostObservation::Cancelled => self.finish(
                    command_id,
                    OperationStage::Cancelled,
                    Some(failure(CoreFailureCode::Cancelled)),
                    None,
                ),
                HostObservation::PlaybackObserved { value } if is_playback_request => {
                    self.accept_playback_observation(
                        observation.request_id,
                        observation.cancellation_id,
                        observation.sequence_number,
                        observation.observed_at.value,
                        value,
                    );
                }
                HostObservation::PlaybackObserved { .. } => {
                    self.fail(command_id, CoreFailureCode::InvalidCommand)
                }
                HostObservation::FeedBytesFetched { .. }
                | HostObservation::FeedNotModified { .. } => {
                    self.fail(command_id, CoreFailureCode::InvalidCommand)
                }
                HostObservation::RecallQueryEmbedded { .. }
                | HostObservation::RecallSpansEmbedded { .. }
                | HostObservation::RecallCandidatesReranked { .. }
                | HostObservation::PublisherChaptersFetched { .. }
                | HostObservation::ChapterModelProviderAccepted { .. }
                | HostObservation::ChapterModelCompleted { .. }
                | HostObservation::ChapterModelFailed { .. }
                | HostObservation::DownloadAccepted { .. }
                | HostObservation::DownloadStaged { .. }
                | HostObservation::DownloadCancelled { .. }
                | HostObservation::DownloadArtifactRemoved { .. }
                | HostObservation::NewEpisodeNotificationDelivered { .. }
                | HostObservation::TranscriptCapabilityObserved { .. }
                | HostObservation::ScheduledAgentExecutionObserved { .. }
                | HostObservation::AgentModelCompleted { .. }
                | HostObservation::AgentApprovalObserved { .. }
                | HostObservation::AgentCapabilityObserved { .. }
                | HostObservation::NostrSignerCredentialReady { .. }
                | HostObservation::NostrEventSigned { .. }
                | HostObservation::NostrSignerCredentialDeleted { .. }
                | HostObservation::CoreWakeReached { .. }
                | HostObservation::LegacyRecallIndexArtifactsRemoved { .. } => {
                    self.fail(command_id, CoreFailureCode::InvalidCommand)
                }
                HostObservation::Unsupported { wire_code } => {
                    self.fail(command_id, CoreFailureCode::Unsupported { wire_code })
                }
            }
        }
        self.trim_operations();
        (true, accepted(request_id))
    }
}

include!("runtime_observations_pending.rs");
