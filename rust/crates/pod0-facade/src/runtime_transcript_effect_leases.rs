use pod0_application::{
    ActivitySubject, ExternalEffectKind, HostRequestEnvelope, LeasedHostRequestEnvelope,
    bounded_host_request_count,
};

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn next_leased_transcript_requests(
        &mut self,
        maximum_count: u16,
    ) -> (bool, Vec<LeasedHostRequestEnvelope>) {
        let maximum = bounded_host_request_count(maximum_count);
        let mut changed = false;
        let mut requests = Vec::with_capacity(maximum);
        while requests.len() < maximum {
            if let Some(store) = &self.store {
                changed |= store
                    .prepare_expired_agent_capability_recovery(self.now())
                    .unwrap_or(false);
            }
            changed |= self.reconcile_download_deadlines();
            changed |= self.prepare_transcript_host_request();
            changed |= self.prepare_model_chapter_host_request();
            let Some(store) = self.store.clone() else {
                break;
            };
            let Ok(Some(lease)) = store.claim_next_effect_with_publisher_limit(
                self.now(),
                120_000,
                pod0_application::MAX_ACTIVE_PUBLISHER_CHAPTER_REQUESTS,
            ) else {
                break;
            };
            changed = true;
            let Some(request) = self.host_request_for_effect(&lease) else {
                continue;
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
            ExternalEffectKind::PublisherChapterProvider
            | ExternalEffectKind::ModelChapterProvider => self.chapter_request_for_effect(lease),
            ExternalEffectKind::Download => self.download_request_for_effect(lease),
            ExternalEffectKind::Playback => self.playback_request_for_effect(lease),
            ExternalEffectKind::FeedNetwork | ExternalEffectKind::Notification => {
                self.feed_request_for_effect(lease)
            }
            ExternalEffectKind::ScheduledAgentProvider => {
                self.scheduled_agent_request_for_effect(lease)
            }
            ExternalEffectKind::CoreWake => self.lifecycle_request_for_effect(lease),
            ExternalEffectKind::Cancellation => self.cancellation_request_for_effect(lease),
            ExternalEffectKind::LibraryNetwork => self.library_network_request_for_effect(lease),
            _ => None,
        }
    }

    fn library_network_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let pod0_application::DurableEffectExecution::LibraryNetwork { request } =
            &lease.request.execution
        else {
            return None;
        };
        (lease.subject
            == ActivitySubject::Operation {
                command_id: request.command_id,
            }
            && lease.request.deadline_at == request.deadline_at)
            .then(|| HostRequestEnvelope {
                request_id: request.request_id,
                command_id: request.command_id,
                cancellation_id: request.cancellation_id,
                issued_revision: request.issued_revision,
                deadline_at: request.deadline_at,
                request: pod0_application::HostRequest::FetchLibraryDocument {
                    workflow_command_id: request.command_id,
                    step: request.step.clone(),
                    url: request.http.url.clone(),
                    accept: request.http.accept.clone(),
                    maximum_response_bytes: request.http.maximum_response_bytes,
                },
            })
    }

    fn cancellation_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let pod0_application::DurableEffectExecution::Cancellation { request } =
            lease.request.execution
        else {
            return None;
        };
        (lease.request.kind == ExternalEffectKind::Cancellation
            && lease.request.deadline_at.is_none())
        .then(|| request.to_host())
    }

    fn lifecycle_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let pod0_application::DurableEffectExecution::Lifecycle { request } =
            &lease.request.execution
        else {
            return None;
        };
        (lease.request.kind == ExternalEffectKind::CoreWake
            && lease.request.deadline_at.is_none()
            && lease.request.episode_id == request_episode_id(request.reason))
        .then(|| request.to_host())
    }

    fn feed_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let pod0_application::DurableEffectExecution::Feed { request } = &lease.request.execution
        else {
            return None;
        };
        let subject_matches = match lease.subject {
            ActivitySubject::Podcast { podcast_id } => {
                request.podcast_id() == podcast_id && request.episode_id().is_none()
            }
            ActivitySubject::Episode { episode_id } => request.episode_id() == Some(episode_id),
            _ => false,
        };
        (subject_matches && request.deadline_at == lease.request.deadline_at)
            .then(|| request.to_host())
    }

    fn playback_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let pod0_application::DurableEffectExecution::Playback { request } =
            &lease.request.execution
        else {
            return None;
        };
        if request.episode_id() != lease.episode_id
            || request.deadline_at != lease.request.deadline_at
        {
            return None;
        }
        Some(request.to_host())
    }

    fn transcript_request_for_effect(
        &self,
        lease: &pod0_storage::EffectLease,
    ) -> Option<HostRequestEnvelope> {
        let ActivitySubject::TranscriptWorkflow { .. } = lease.subject else {
            return None;
        };
        let pod0_application::DurableEffectExecution::Transcript { request } =
            &lease.request.execution
        else {
            return None;
        };
        (lease.episode_id == Some(request.capability.context().episode_id)
            && lease.request.deadline_at == request.deadline_at)
            .then(|| pod0_application::HostRequestEnvelope {
                request_id: request.request_id,
                command_id: request.command_id,
                cancellation_id: request.cancellation_id,
                issued_revision: request.issued_revision,
                deadline_at: request.deadline_at,
                request: pod0_application::HostRequest::ExecuteTranscriptCapability {
                    capability: request.capability.clone(),
                },
            })
    }
}

fn request_episode_id(reason: pod0_application::CoreWakeReason) -> Option<pod0_domain::EpisodeId> {
    match reason {
        pod0_application::CoreWakeReason::ModelChapterRetry { episode_id, .. }
        | pod0_application::CoreWakeReason::TranscriptProviderRecovery { episode_id, .. }
        | pod0_application::CoreWakeReason::TranscriptRetry { episode_id, .. }
        | pod0_application::CoreWakeReason::FeedDiscoveryNotificationRetry { episode_id, .. } => {
            Some(episode_id)
        }
        pod0_application::CoreWakeReason::ModelChapterFinalization { .. }
        | pod0_application::CoreWakeReason::TranscriptFinalization { .. }
        | pod0_application::CoreWakeReason::FeedFetchRetry { .. }
        | pod0_application::CoreWakeReason::Unsupported { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod0_domain::*;

    #[test]
    fn transcript_lease_maps_only_its_exact_persisted_payload() {
        let episode_id = EpisodeId::from_parts(213, 1);
        let workflow_id = TranscriptWorkflowId::from_parts(213, 2);
        let exact = pod0_application::DurableTranscriptEffectRequest {
            request_id: HostRequestId::from_parts(213, 3),
            command_id: CommandId::from_parts(213, 4),
            cancellation_id: CancellationId::from_parts(213, 5),
            issued_revision: StateRevision::new(6),
            deadline_at: Some(UnixTimestampMilliseconds::new(2_000)),
            capability: pod0_application::TranscriptCapabilityRequest::FetchPublisher {
                context: pod0_application::TranscriptCapabilityContext {
                    episode_id,
                    podcast_id: PodcastId::from_parts(213, 7),
                    source_revision: "immutable-source".to_owned(),
                },
                source_url: "https://example.com/transcript.vtt".to_owned(),
                mime_hint: Some("text/vtt".to_owned()),
                maximum_response_bytes: 100_000,
            },
        };
        let mut lease = pod0_storage::EffectLease {
            intent_id: EffectIntentId::from_parts(213, 8),
            attempt_id: EffectAttemptId::from_parts(213, 9),
            lease_id: EffectLeaseId::from_parts(213, 10),
            fence: 1,
            authorizing_activity_id: ActivityId::from_parts(213, 11),
            correlation_id: ActivityCorrelationId::from_parts(213, 12),
            subject: ActivitySubject::TranscriptWorkflow { workflow_id },
            episode_id: Some(episode_id),
            request: pod0_application::DurableExternalEffectRequest {
                kind: ExternalEffectKind::TranscriptProvider,
                subject: ActivitySubject::TranscriptWorkflow { workflow_id },
                episode_id: Some(episode_id),
                not_before: None,
                deadline_at: exact.deadline_at,
                execution: pod0_application::DurableEffectExecution::Transcript { request: exact },
            },
            expires_at: UnixTimestampMilliseconds::new(3_000),
        };
        let state = FacadeState::default();
        assert!(matches!(
            state.transcript_request_for_effect(&lease).unwrap().request,
            pod0_application::HostRequest::ExecuteTranscriptCapability { capability:
                pod0_application::TranscriptCapabilityRequest::FetchPublisher { source_url, .. } }
                if source_url == "https://example.com/transcript.vtt"
        ));
        lease.episode_id = Some(EpisodeId::from_parts(213, 99));
        assert!(state.transcript_request_for_effect(&lease).is_none());
    }
}
