use pod0_facade::{
    AgentTurnStage, CoreFailureCode, DurableExternalEffectRequest, ExternalEffectKind,
    OperationProjection, OperationStage, Pod0Facade, Projection,
};

use crate::protocol::{CliError, OperationDto, PendingHostWorkDto};

#[allow(clippy::type_complexity)]
pub(crate) fn library(
    facade: &Pod0Facade,
    offset: u32,
    max_items: u16,
) -> Result<
    (
        pod0_facade::LibraryProjection,
        (usize, usize, usize),
        Vec<pod0_facade::PodcastId>,
    ),
    CliError,
> {
    let (snapshot, totals, subscribed_podcast_ids) =
        facade.library_page_with_totals(offset, max_items);
    match snapshot.projection {
        Projection::Library { value } => Ok((value, totals, subscribed_podcast_ids)),
        _ => Err(projection_error()),
    }
}

pub(crate) fn find_operation(
    operations: &[OperationProjection],
    command_id: pod0_facade::CommandId,
) -> Result<&OperationProjection, CliError> {
    operations
        .iter()
        .find(|operation| operation.command_id == command_id)
        .ok_or_else(|| {
            CliError::new(
                "operation_missing",
                "command operation was not projected",
                true,
            )
        })
}

pub(crate) fn operation_dto(operation: &OperationProjection) -> OperationDto {
    OperationDto {
        stage: operation_stage(operation.stage).to_owned(),
        failure_code: operation
            .failure
            .as_ref()
            .map(|value| failure_code(value.code)),
        safe_detail: operation
            .failure
            .as_ref()
            .and_then(|value| value.safe_detail.clone()),
    }
}

pub(crate) fn operation_error(operation: &OperationProjection) -> CliError {
    if let Some(failure) = &operation.failure {
        CliError::new(
            &failure_code(failure.code),
            failure
                .safe_detail
                .clone()
                .unwrap_or_else(|| "core command failed".to_owned()),
            !matches!(failure.retryability, pod0_facade::Retryability::Never),
        )
    } else {
        CliError::new(
            "operation_incomplete",
            format!("operation is {}", operation_stage(operation.stage)),
            true,
        )
    }
}

pub(crate) fn agent_stage(stage: AgentTurnStage) -> &'static str {
    match stage {
        AgentTurnStage::AwaitingModel => "awaiting_model",
        AgentTurnStage::ApprovalRequired => "approval_required",
        AgentTurnStage::Authorized => "authorized",
        AgentTurnStage::Executing => "executing",
        AgentTurnStage::CommitPending => "commit_pending",
        AgentTurnStage::Committed => "committed",
        AgentTurnStage::Completed => "completed",
        AgentTurnStage::Denied => "denied",
        AgentTurnStage::Cancelled => "cancelled",
        AgentTurnStage::Blocked => "blocked",
        AgentTurnStage::OutcomeAmbiguous => "outcome_ambiguous",
        AgentTurnStage::Failed => "failed",
    }
}

pub(crate) fn pending_work(effect: DurableExternalEffectRequest) -> PendingHostWorkDto {
    PendingHostWorkDto {
        kind: effect_kind(effect.kind).to_owned(),
        not_before_milliseconds: effect.not_before.map(|value| value.value),
        deadline_milliseconds: effect.deadline_at.map(|value| value.value),
    }
}

pub(crate) fn open_error(error: pod0_facade::FacadeOpenError) -> CliError {
    CliError::new("store_open_failed", error.to_string(), false)
}

pub(crate) fn projection_error() -> CliError {
    CliError::new(
        "projection_unavailable",
        "the requested projection is unavailable",
        true,
    )
}

fn operation_stage(stage: OperationStage) -> &'static str {
    match stage {
        OperationStage::Accepted => "accepted",
        OperationStage::Running => "running",
        OperationStage::Blocked => "blocked",
        OperationStage::Failed => "failed",
        OperationStage::Cancelled => "cancelled",
        OperationStage::Succeeded => "succeeded",
        OperationStage::Unsupported { .. } => "unsupported",
    }
}

fn failure_code(code: CoreFailureCode) -> String {
    match code {
        CoreFailureCode::InvalidCommand => "invalid_command",
        CoreFailureCode::InvalidFeedUrl => "invalid_feed_url",
        CoreFailureCode::FeedMalformed => "feed_malformed",
        CoreFailureCode::AlreadySubscribed => "already_subscribed",
        CoreFailureCode::StorageUnavailable => "storage_unavailable",
        CoreFailureCode::RevisionConflict => "revision_conflict",
        CoreFailureCode::NotFound => "not_found",
        CoreFailureCode::InvalidMemory => "invalid_memory",
        CoreFailureCode::InvalidNote => "invalid_note",
        CoreFailureCode::InvalidClip => "invalid_clip",
        CoreFailureCode::InvalidTranscript => "invalid_transcript",
        CoreFailureCode::InvalidChapter => "invalid_chapter",
        CoreFailureCode::HostUnavailable => "host_unavailable",
        CoreFailureCode::Unauthorized => "unauthorized",
        CoreFailureCode::HostRejected => "host_rejected",
        CoreFailureCode::Cancelled => "cancelled",
        CoreFailureCode::Unsupported { .. } => "unsupported",
    }
    .to_owned()
}

fn effect_kind(kind: ExternalEffectKind) -> &'static str {
    match kind {
        ExternalEffectKind::FeedNetwork => "feed_network",
        ExternalEffectKind::Playback => "playback",
        ExternalEffectKind::RecallProvider => "recall_provider",
        ExternalEffectKind::ChapterProvider => "chapter_provider",
        ExternalEffectKind::Download => "download",
        ExternalEffectKind::Notification => "notification",
        ExternalEffectKind::TranscriptProvider => "transcript_provider",
        ExternalEffectKind::AgentProvider => "agent_provider",
        ExternalEffectKind::AgentApproval => "agent_approval",
        ExternalEffectKind::AgentCapability => "agent_capability",
        ExternalEffectKind::ScheduledAgentProvider => "scheduled_agent_provider",
        ExternalEffectKind::CoreWake => "core_wake",
        ExternalEffectKind::Filesystem => "filesystem",
        ExternalEffectKind::Publication => "publication",
        ExternalEffectKind::PublisherChapterProvider => "publisher_chapter_provider",
        ExternalEffectKind::ModelChapterProvider => "model_chapter_provider",
        ExternalEffectKind::Cancellation => "cancellation",
        ExternalEffectKind::LibraryNetwork => "library_network",
    }
}
