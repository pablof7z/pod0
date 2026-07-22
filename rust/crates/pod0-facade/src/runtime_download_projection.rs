use pod0_application::{
    CoreFailureCode, DownloadDesiredState, DownloadIntentOrigin, DownloadWorkflowAllowedActions,
    DownloadWorkflowFailure, DownloadWorkflowFailureCode, DownloadWorkflowProjection,
    DownloadWorkflowStage, DownloadWorkflowsProjection,
};
use pod0_domain::{EpisodeId, UnixTimestampMilliseconds};
use pod0_storage::{
    DownloadWorkflowRecord, StoredDownloadDesiredState, StoredDownloadOrigin, StoredDownloadStage,
};

use crate::runtime_state::{FacadeState, failure};

impl FacadeState {
    pub(super) fn download_workflows_projection(
        &self,
        episode_id: Option<EpisodeId>,
        offset: u32,
        max_items: u16,
    ) -> DownloadWorkflowsProjection {
        let Some(store) = &self.store else {
            return unavailable();
        };
        match store.download_workflow_page(episode_id, offset, max_items) {
            Ok(page) => DownloadWorkflowsProjection {
                workflows: page.items.into_iter().map(project).collect(),
                has_more: page.has_more,
                failure: None,
            },
            Err(_) => unavailable(),
        }
    }
}

fn unavailable() -> DownloadWorkflowsProjection {
    DownloadWorkflowsProjection {
        workflows: Vec::new(),
        has_more: false,
        failure: Some(failure(CoreFailureCode::StorageUnavailable)),
    }
}

fn project(record: DownloadWorkflowRecord) -> DownloadWorkflowProjection {
    let stage = stage(record.stage);
    let has_artifact = record.artifact_key.is_some();
    DownloadWorkflowProjection {
        episode_id: record.episode_id,
        intent_id: record.intent_id,
        input_version: record.input_version,
        origin: origin(record.origin),
        desired_state: match record.desired_state {
            StoredDownloadDesiredState::Present => DownloadDesiredState::Present,
            StoredDownloadDesiredState::Absent => DownloadDesiredState::Absent,
        },
        stage,
        workflow_revision: record.workflow_revision,
        attempt: record.attempt,
        attempt_id: record.attempt_id,
        request_id: record.request_id,
        not_before: record.not_before_ms.map(UnixTimestampMilliseconds::new),
        failure: record
            .failure_code
            .as_deref()
            .map(|code| DownloadWorkflowFailure {
                code: failure_code(code),
                safe_detail: record.failure_detail,
                retryable: record.failure_retryable,
            }),
        updated_at: UnixTimestampMilliseconds::new(record.updated_at_ms),
        allowed_actions: DownloadWorkflowAllowedActions {
            can_retry: matches!(
                stage,
                DownloadWorkflowStage::Failed | DownloadWorkflowStage::Cancelled
            ),
            can_cancel: matches!(
                stage,
                DownloadWorkflowStage::WaitingForEnvironment
                    | DownloadWorkflowStage::Requested
                    | DownloadWorkflowStage::HostAccepted
                    | DownloadWorkflowStage::Transferring
                    | DownloadWorkflowStage::RetryScheduled
            ),
            can_remove: stage == DownloadWorkflowStage::Succeeded
                || (stage == DownloadWorkflowStage::Failed && has_artifact),
        },
    }
}

fn origin(value: StoredDownloadOrigin) -> DownloadIntentOrigin {
    match value {
        StoredDownloadOrigin::User => DownloadIntentOrigin::User,
        StoredDownloadOrigin::Playback => DownloadIntentOrigin::Playback,
        StoredDownloadOrigin::Automatic => DownloadIntentOrigin::Automatic,
        StoredDownloadOrigin::Unsupported(wire_code) => {
            DownloadIntentOrigin::Unsupported { wire_code }
        }
    }
}

fn stage(value: StoredDownloadStage) -> DownloadWorkflowStage {
    match value {
        StoredDownloadStage::Waiting => DownloadWorkflowStage::WaitingForEnvironment,
        StoredDownloadStage::Requested => DownloadWorkflowStage::Requested,
        StoredDownloadStage::HostAccepted => DownloadWorkflowStage::HostAccepted,
        StoredDownloadStage::Transferring => DownloadWorkflowStage::Transferring,
        StoredDownloadStage::Staged => DownloadWorkflowStage::Staged,
        StoredDownloadStage::RetryScheduled => DownloadWorkflowStage::RetryScheduled,
        StoredDownloadStage::Removing => DownloadWorkflowStage::Removing,
        StoredDownloadStage::Cancelled => DownloadWorkflowStage::Cancelled,
        StoredDownloadStage::Failed => DownloadWorkflowStage::Failed,
        StoredDownloadStage::Succeeded => DownloadWorkflowStage::Succeeded,
    }
}

fn failure_code(value: &str) -> DownloadWorkflowFailureCode {
    match value {
        "offline" | "network_unknown" => DownloadWorkflowFailureCode::Offline,
        "wifi_required" => DownloadWorkflowFailureCode::WifiRequired,
        "insufficient_storage" => DownloadWorkflowFailureCode::InsufficientStorage,
        "missing_episode" => DownloadWorkflowFailureCode::MissingEpisode,
        "invalid_enclosure" => DownloadWorkflowFailureCode::InvalidEnclosure,
        "stale_input" => DownloadWorkflowFailureCode::StaleInput,
        "host_rejected" => DownloadWorkflowFailureCode::HostRejected,
        "transport" => DownloadWorkflowFailureCode::Transport,
        "timed_out" => DownloadWorkflowFailureCode::TimedOut,
        "permission_denied" => DownloadWorkflowFailureCode::PermissionDenied,
        "invalid_artifact" => DownloadWorkflowFailureCode::InvalidArtifact,
        "storage_unavailable" => DownloadWorkflowFailureCode::StorageUnavailable,
        "cancelled" => DownloadWorkflowFailureCode::Cancelled,
        "retry_exhausted" => DownloadWorkflowFailureCode::RetryExhausted,
        _ => DownloadWorkflowFailureCode::Unsupported { wire_code: 1 },
    }
}
