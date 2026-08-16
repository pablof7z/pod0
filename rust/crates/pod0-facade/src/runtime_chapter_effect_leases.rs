use pod0_application::{
    ActivitySubject, DurableEffectExecution, DurableModelChapterAction, ExternalEffectKind,
    HostRequest, HostRequestEnvelope,
};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn chapter_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let ActivitySubject::Episode { episode_id } = lease.subject else {
            return None;
        };
        if lease.episode_id != Some(episode_id) {
            return None;
        }
        match (&lease.request.kind, &lease.request.execution) {
            (
                ExternalEffectKind::PublisherChapterProvider,
                DurableEffectExecution::PublisherChapter { request },
            ) if request.episode_id == episode_id
                && request.deadline_at == lease.request.deadline_at
                && request.not_before == lease.request.not_before =>
            {
                Some(HostRequestEnvelope {
                    request_id: request.request_id,
                    command_id: request.command_id,
                    cancellation_id: request.cancellation_id,
                    issued_revision: request.issued_revision,
                    deadline_at: request.deadline_at,
                    request: HostRequest::FetchPublisherChapters {
                        episode_id,
                        source_url: request.source_url.clone(),
                        not_before: request.not_before,
                        maximum_response_bytes: request.maximum_response_bytes,
                    },
                })
            }
            (
                ExternalEffectKind::ModelChapterProvider,
                DurableEffectExecution::ModelChapter { request },
            ) if request.episode_id == episode_id
                && request.deadline_at == lease.request.deadline_at =>
            {
                Some(HostRequestEnvelope {
                    request_id: request.request_id,
                    command_id: request.command_id,
                    cancellation_id: request.cancellation_id,
                    issued_revision: request.issued_revision,
                    deadline_at: request.deadline_at,
                    request: match &request.action {
                        DurableModelChapterAction::Execute { execution } => {
                            HostRequest::ExecuteChapterModel {
                                episode_id,
                                generation: request.generation,
                                submission_fence_id: request.submission_fence_id,
                                execution: execution.clone(),
                            }
                        }
                        DurableModelChapterAction::Recover {
                            provider,
                            model,
                            provider_operation_id,
                            provider_status,
                            maximum_completion_bytes,
                        } => HostRequest::RecoverChapterModelOperation {
                            episode_id,
                            generation: request.generation,
                            submission_fence_id: request.submission_fence_id,
                            provider: provider.clone(),
                            model: model.clone(),
                            provider_operation_id: provider_operation_id.clone(),
                            provider_status: provider_status.clone(),
                            maximum_completion_bytes: *maximum_completion_bytes,
                        },
                    },
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod0_domain::*;

    #[test]
    fn exact_publisher_payload_maps_without_workflow_state_and_drift_fails_closed() {
        let episode_id = EpisodeId::from_parts(219, 1);
        let request = pod0_application::DurablePublisherChapterEffectRequest {
            request_id: HostRequestId::from_parts(219, 2),
            command_id: CommandId::from_parts(219, 3),
            cancellation_id: CancellationId::from_parts(219, 4),
            issued_revision: StateRevision::new(5),
            deadline_at: Some(UnixTimestampMilliseconds::new(2_000)),
            episode_id,
            source_url: "https://example.com/chapters.json".to_owned(),
            not_before: None,
            maximum_response_bytes: 10_000,
        };
        let mut lease = lease(
            episode_id,
            ExternalEffectKind::PublisherChapterProvider,
            DurableEffectExecution::PublisherChapter { request },
        );
        let state = FacadeState::default();
        assert!(matches!(
            state.chapter_request_for_effect(&lease).unwrap().request,
            HostRequest::FetchPublisherChapters { source_url, .. }
                if source_url == "https://example.com/chapters.json"
        ));
        lease.request.deadline_at = Some(UnixTimestampMilliseconds::new(2_001));
        assert!(state.chapter_request_for_effect(&lease).is_none());
    }

    fn lease(
        episode_id: EpisodeId,
        kind: ExternalEffectKind,
        execution: DurableEffectExecution,
    ) -> pod0_storage::EffectLease {
        pod0_storage::EffectLease {
            intent_id: EffectIntentId::from_parts(219, 5),
            attempt_id: EffectAttemptId::from_parts(219, 6),
            lease_id: EffectLeaseId::from_parts(219, 7),
            fence: 1,
            authorizing_activity_id: ActivityId::from_parts(219, 8),
            correlation_id: ActivityCorrelationId::from_parts(219, 9),
            subject: ActivitySubject::Episode { episode_id },
            episode_id: Some(episode_id),
            request: pod0_application::DurableExternalEffectRequest {
                kind,
                subject: ActivitySubject::Episode { episode_id },
                episode_id: Some(episode_id),
                not_before: None,
                deadline_at: Some(UnixTimestampMilliseconds::new(2_000)),
                execution,
            },
            expires_at: UnixTimestampMilliseconds::new(3_000),
        }
    }
}
