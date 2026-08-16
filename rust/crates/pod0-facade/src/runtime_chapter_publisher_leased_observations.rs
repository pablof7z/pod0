use pod0_application::{
    ChapterObservationProjection, ChapterObservationRejection, HostObservation,
    HostObservationReceipt, HostObservationRejection, LeasedHostObservationEnvelope,
    OperationStage, PublisherChapterObservation, qualify_publisher_chapter_observation,
};
use pod0_domain::ContentDigest;
use pod0_storage::{
    PublisherChapterObservationAction, PublisherChapterObservationCommitInput,
    PublisherChapterWorkflowRecord, PublisherChapterWorkflowState,
};
use sha2::{Digest as _, Sha256};

use crate::runtime_chapter_model_receipts::{persisted, rejected, retain, storage_receipt};
use crate::runtime_chapter_publisher_observation_failure::failure_action;
use crate::runtime_chapter_workflow_observations::{
    FAILURE_INVALID_DOCUMENT, FAILURE_INVALID_RESPONSE, FAILURE_NOT_FOUND,
    FAILURE_RESPONSE_TOO_LARGE, FAILURE_TRANSPORT, core_failure_for_workflow, host_failure,
};
use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn record_leased_publisher_chapter_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let acceptance = self.host_requests.validate_observation(&leased.observation);
        if acceptance == pod0_application::ObservationAcceptance::PayloadTooLarge {
            return self.record_oversized_publisher_observation(leased);
        }
        if !matches!(
            acceptance,
            pod0_application::ObservationAcceptance::Accepted
                | pod0_application::ObservationAcceptance::UnknownRequest
                | pod0_application::ObservationAcceptance::Duplicate
        ) {
            return (
                false,
                crate::runtime_observation_mapping::rejected(request_id, acceptance),
            );
        }
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        if self.reload_listening().is_err() {
            return (false, retain(request_id));
        }
        let record =
            match store.publisher_chapter_workflow_for_effect_intent(leased.lease.intent_id) {
                Ok(Some(record)) if record.request_id == Some(request_id) => record,
                Ok(_) => {
                    return (
                        false,
                        rejected(request_id, HostObservationRejection::StaleWorkflow),
                    );
                }
                Err(_) => return (false, retain(request_id)),
            };
        let action = if self.publisher_source_is_current(&record) {
            match self.publisher_action(&record, &leased.observation.observation) {
                Ok(action) => action,
                Err(receipt) => return (false, receipt),
            }
        } else {
            PublisherChapterObservationAction::Supersede
        };
        let committed = match store.commit_publisher_chapter_observation(
            PublisherChapterObservationCommitInput {
                lease: leased.lease,
                observation: leased.observation,
                action: action.clone(),
                committed_at: self.now(),
            },
        ) {
            Ok(value) => value,
            Err(error) => return (false, storage_receipt(request_id, error)),
        };
        if committed.replayed {
            return (
                false,
                rejected(request_id, HostObservationRejection::Duplicate),
            );
        }
        self.retire_publisher_chapter_request(request_id);
        self.apply_publisher_observation_projection(&record, &committed.workflow, &action);
        if matches!(action, PublisherChapterObservationAction::Supersede) {
            let _ = self.start_publisher_chapter_workflow(
                record.episode_id,
                record.cancellation_id,
                record.command_id,
                false,
                true,
            );
        }
        self.advance_revision();
        let terminal = !matches!(
            committed.workflow.state,
            PublisherChapterWorkflowState::Requested
                | PublisherChapterWorkflowState::RetryScheduled
        );
        (true, persisted(request_id, terminal))
    }

    fn record_oversized_publisher_observation(
        &mut self,
        mut leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        leased.observation.observation = HostObservation::Failed {
            code: pod0_application::HostFailureCode::ResponseTooLarge,
            safe_detail: None,
        };
        self.record_leased_publisher_chapter_observation(leased)
    }

    fn publisher_action(
        &self,
        record: &PublisherChapterWorkflowRecord,
        observation: &HostObservation,
    ) -> Result<PublisherChapterObservationAction, HostObservationReceipt> {
        let action = match observation {
            HostObservation::PublisherChaptersFetched {
                bytes,
                content_type,
                response_url,
                http_status,
                ..
            } if (200..300).contains(http_status) => {
                let episode = self
                    .listening
                    .episodes
                    .iter()
                    .find(|episode| episode.episode_id == record.episode_id)
                    .ok_or_else(|| {
                        rejected(
                            record.request_id.expect("active publisher request"),
                            HostObservationRejection::StaleWorkflow,
                        )
                    })?;
                let projected =
                    qualify_publisher_chapter_observation(PublisherChapterObservation {
                        episode_id: episode.episode_id,
                        podcast_id: episode.podcast_id,
                        resolved_source_url: response_url.clone(),
                        content_type: content_type.clone(),
                        payload_digest: ContentDigest::from_bytes(Sha256::digest(bytes).into()),
                        payload: bytes.clone(),
                        generated_at: self.now(),
                        duration_milliseconds: episode.duration_milliseconds,
                    });
                match projected {
                    ChapterObservationProjection::Qualified { artifact, .. } => {
                        let sealed = pod0_domain::ChapterArtifact::seal(artifact.clone())
                            .map_err(|_| retain(record.request_id.expect("publisher request")))?;
                        let selected = self
                            .store
                            .as_ref()
                            .ok_or_else(|| retain(record.request_id.expect("publisher request")))?
                            .selected_chapter_artifact(record.episode_id)
                            .map_err(|_| retain(record.request_id.expect("publisher request")))?;
                        if selected.as_ref().map(|value| value.artifact.artifact_id)
                            != Some(sealed.artifact_id)
                            && selected
                                .as_ref()
                                .map_or(pod0_domain::StateRevision::INITIAL, |value| {
                                    value.selection_revision
                                })
                                != record.expected_selection_revision
                        {
                            failure_action(
                                record,
                                crate::runtime_chapter_workflow_observations::FAILURE_SELECTION_CHANGED,
                                false,
                                self.now().value,
                                self.revision,
                            )
                        } else {
                            PublisherChapterObservationAction::Complete { artifact }
                        }
                    }
                    ChapterObservationProjection::Rejected { reason } => failure_action(
                        record,
                        if reason == ChapterObservationRejection::PayloadTooLarge {
                            FAILURE_RESPONSE_TOO_LARGE
                        } else {
                            FAILURE_INVALID_DOCUMENT
                        },
                        false,
                        self.now().value,
                        self.revision,
                    ),
                }
            }
            HostObservation::PublisherChaptersFetched {
                http_status: 404 | 410,
                ..
            } => failure_action(
                record,
                FAILURE_NOT_FOUND,
                false,
                self.now().value,
                self.revision,
            ),
            HostObservation::PublisherChaptersFetched { http_status, .. } => {
                let retryable = matches!(http_status, 408 | 425 | 429) || *http_status >= 500;
                failure_action(
                    record,
                    if retryable {
                        FAILURE_TRANSPORT
                    } else {
                        FAILURE_INVALID_RESPONSE
                    },
                    retryable,
                    self.now().value,
                    self.revision,
                )
            }
            HostObservation::Failed { code, .. } => {
                let (failure_code, retryable) = host_failure(*code);
                failure_action(
                    record,
                    failure_code,
                    retryable,
                    self.now().value,
                    self.revision,
                )
            }
            HostObservation::Cancelled => PublisherChapterObservationAction::Cancel,
            HostObservation::Unsupported { .. } => failure_action(
                record,
                FAILURE_INVALID_RESPONSE,
                false,
                self.now().value,
                self.revision,
            ),
            _ => {
                return Err(rejected(
                    record.request_id.expect("active publisher request"),
                    HostObservationRejection::MismatchedPayload,
                ));
            }
        };
        Ok(action)
    }

    fn apply_publisher_observation_projection(
        &mut self,
        original: &PublisherChapterWorkflowRecord,
        updated: &PublisherChapterWorkflowRecord,
        action: &PublisherChapterObservationAction,
    ) {
        match action {
            PublisherChapterObservationAction::Complete { .. } => match self.reload_listening() {
                Ok(()) => self.succeed(original.command_id, None),
                Err(error) => self.fail(
                    original.command_id,
                    crate::runtime_storage_commands::storage_failure(error),
                ),
            },
            PublisherChapterObservationAction::Fail { failure, .. } => {
                if updated.state == PublisherChapterWorkflowState::RetryScheduled {
                    self.finish(original.command_id, OperationStage::Running, None, None);
                } else {
                    self.fail(
                        original.command_id,
                        core_failure_for_workflow(&failure.failure_code),
                    );
                }
            }
            PublisherChapterObservationAction::Cancel => self.finish(
                original.command_id,
                OperationStage::Cancelled,
                Some(failure(pod0_application::CoreFailureCode::Cancelled)),
                None,
            ),
            PublisherChapterObservationAction::Supersede => {}
        }
    }
}
