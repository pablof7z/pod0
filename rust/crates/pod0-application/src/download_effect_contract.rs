use pod0_domain::{
    CancellationId, CommandId, DownloadAttemptId, DownloadIntentId, EpisodeId, HostRequestId,
    StateRevision, UnixTimestampMilliseconds,
};

use crate::{HostRequest, HostRequestEnvelope};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DurableDownloadEffectRequest {
    pub request_id: HostRequestId,
    pub command_id: CommandId,
    pub cancellation_id: CancellationId,
    pub issued_revision: StateRevision,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub deadline_at: Option<UnixTimestampMilliseconds>,
    pub action: DurableDownloadEffectAction,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DurableDownloadEffectAction {
    Start {
        episode_id: EpisodeId,
        intent_id: DownloadIntentId,
        attempt_id: DownloadAttemptId,
        input_version: String,
        enclosure_url: String,
        resume_key: Option<String>,
    },
    Cancel {
        episode_id: EpisodeId,
        intent_id: DownloadIntentId,
        attempt_id: DownloadAttemptId,
        external_task_key: Option<String>,
    },
    Remove {
        episode_id: EpisodeId,
        artifact_key: String,
    },
}

impl DurableDownloadEffectRequest {
    #[must_use]
    pub const fn episode_id(&self) -> EpisodeId {
        match self.action {
            DurableDownloadEffectAction::Start { episode_id, .. }
            | DurableDownloadEffectAction::Cancel { episode_id, .. }
            | DurableDownloadEffectAction::Remove { episode_id, .. } => episode_id,
        }
    }

    #[must_use]
    pub fn to_host(&self) -> HostRequestEnvelope {
        let request = match &self.action {
            DurableDownloadEffectAction::Start {
                episode_id,
                intent_id,
                attempt_id,
                input_version,
                enclosure_url,
                resume_key,
            } => HostRequest::StartEpisodeDownload {
                episode_id: *episode_id,
                intent_id: *intent_id,
                attempt_id: *attempt_id,
                input_version: input_version.clone(),
                enclosure_url: enclosure_url.clone(),
                resume_key: resume_key.clone(),
            },
            DurableDownloadEffectAction::Cancel {
                episode_id,
                intent_id,
                attempt_id,
                external_task_key,
            } => HostRequest::CancelEpisodeDownload {
                episode_id: *episode_id,
                intent_id: *intent_id,
                attempt_id: *attempt_id,
                external_task_key: external_task_key.clone(),
            },
            DurableDownloadEffectAction::Remove {
                episode_id,
                artifact_key,
            } => HostRequest::RemoveEpisodeDownloadArtifact {
                episode_id: *episode_id,
                artifact_key: artifact_key.clone(),
            },
        };
        HostRequestEnvelope {
            request_id: self.request_id,
            command_id: self.command_id,
            cancellation_id: self.cancellation_id,
            issued_revision: self.issued_revision,
            deadline_at: self.deadline_at,
            request,
        }
    }
}
