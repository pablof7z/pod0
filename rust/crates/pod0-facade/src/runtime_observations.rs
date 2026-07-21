use pod0_application::{
    CoreFailureCode, HostFailureCode, HostObservation, HostObservationEnvelope,
    HostObservationReceipt, HostObservationRejection, ObservationAcceptance, OperationStage,
};

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn record_host_observation(
        &mut self,
        observation: HostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = observation.request_id;
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
        let pending_wake = self.pending_core_wakes.contains_key(&request_id);
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
        if let Some(record) = pending_model {
            let retained = observation.clone();
            let receipt = self.persist_model_observation(record, observation);
            if matches!(receipt, HostObservationReceipt::RetainAndRetry { .. }) {
                self.pending_model_observations.insert(request_id, retained);
            }
            let changed = matches!(receipt, HostObservationReceipt::Persisted { .. });
            return (changed, receipt);
        }
        if pending_wake {
            let changed = self.finish_core_wake(request_id, observation.observation);
            return (changed, accepted(request_id));
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
            self.pending_recalls.remove(&observation.request_id);
            self.finish_recall_observation(pending, observation.observation);
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

    pub(super) fn retry_pending_publisher_observations(&mut self) -> bool {
        let request_ids = self
            .pending_publisher_observations
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut changed = false;
        for request_id in request_ids {
            let Some(record) = self.pending_publisher_chapters.get(&request_id).cloned() else {
                self.pending_publisher_observations.remove(&request_id);
                continue;
            };
            let Some(observation) = self
                .pending_publisher_observations
                .get(&request_id)
                .cloned()
            else {
                continue;
            };
            if self.finish_publisher_chapter_observation(record, observation) {
                self.retire_publisher_chapter_request(request_id);
                changed = true;
            }
        }
        if changed {
            self.trim_operations();
        }
        changed
    }

    fn late_ambiguous_model_record(
        &self,
        observation: &HostObservationEnvelope,
    ) -> Option<pod0_storage::ModelChapterWorkflowRecord> {
        let episode_id = match observation.observation {
            HostObservation::ChapterModelProviderAccepted { episode_id, .. }
            | HostObservation::ChapterModelCompleted { episode_id, .. }
            | HostObservation::ChapterModelFailed { episode_id, .. } => episode_id,
            _ => return None,
        };
        let record = self
            .store
            .as_ref()?
            .model_chapter_workflow(episode_id)
            .ok()??;
        (record.state == pod0_storage::ModelChapterWorkflowState::Ambiguous
            && record.request_id == Some(observation.request_id)
            && record.cancellation_id == observation.cancellation_id
            && record.issued_revision == observation.observed_request_revision)
            .then_some(record)
    }
}

pub(super) fn host_failure(code: HostFailureCode) -> CoreFailureCode {
    match code {
        HostFailureCode::PermissionDenied => CoreFailureCode::HostRejected,
        HostFailureCode::InvalidResponse | HostFailureCode::ResponseTooLarge => {
            CoreFailureCode::FeedMalformed
        }
        _ => CoreFailureCode::HostUnavailable,
    }
}

fn accepted(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::AcceptedTransient { request_id }
}

fn retain(request_id: pod0_domain::HostRequestId) -> HostObservationReceipt {
    HostObservationReceipt::RetainAndRetry { request_id }
}

fn rejected(
    request_id: pod0_domain::HostRequestId,
    acceptance: ObservationAcceptance,
) -> HostObservationReceipt {
    let reason = match acceptance {
        ObservationAcceptance::UnknownRequest => HostObservationRejection::UnknownRequest,
        ObservationAcceptance::Duplicate => HostObservationRejection::Duplicate,
        ObservationAcceptance::Cancelled => HostObservationRejection::Cancelled,
        ObservationAcceptance::CancellationMismatch => {
            HostObservationRejection::CancellationMismatch
        }
        ObservationAcceptance::StaleRequestRevision => {
            HostObservationRejection::StaleRequestRevision
        }
        ObservationAcceptance::OutOfOrder => HostObservationRejection::OutOfOrder,
        ObservationAcceptance::MismatchedPayload => HostObservationRejection::MismatchedPayload,
        ObservationAcceptance::PayloadTooLarge => HostObservationRejection::PayloadTooLarge,
        ObservationAcceptance::Accepted => unreachable!("accepted observations are handled above"),
    };
    HostObservationReceipt::Rejected { request_id, reason }
}
