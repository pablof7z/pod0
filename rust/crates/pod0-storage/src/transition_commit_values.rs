use pod0_application::{ActivityDomain, ExternalEffectKind};

fn id_bytes(value: &[u8]) -> Result<[u8; 16], StorageError> {
    value.try_into().map_err(|_| StorageError::InvalidActivity)
}

fn sequence(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::InvalidActivity)
}

const fn disposition_code(value: RequestDisposition) -> u8 {
    match value {
        RequestDisposition::Accepted => 1,
        RequestDisposition::Rejected { .. } => 2,
        RequestDisposition::Stale => 3,
        RequestDisposition::Duplicate => 4,
        RequestDisposition::AlreadyComplete => 5,
        RequestDisposition::NoSemanticChange => 6,
    }
}

fn decode_disposition(code: u8, payload: &str) -> Result<RequestDisposition, StorageError> {
    let disposition = serde_json::from_str(payload).map_err(|_| StorageError::InvalidActivity)?;
    (disposition_code(disposition) == code)
        .then_some(disposition)
        .ok_or(StorageError::InvalidActivity)
}

fn subject(value: ActivitySubject) -> (u8, Option<[u8; 16]>) {
    match value {
        ActivitySubject::Global => (0, None),
        ActivitySubject::Podcast { podcast_id } => (1, Some(podcast_id.into_bytes())),
        ActivitySubject::Episode { episode_id } => (2, Some(episode_id.into_bytes())),
        ActivitySubject::Conversation { conversation_id } => {
            (3, Some(conversation_id.into_bytes()))
        }
        ActivitySubject::AgentTurn { turn_id } => (4, Some(turn_id.into_bytes())),
        ActivitySubject::ScheduledOccurrence { occurrence_id } => {
            (5, Some(occurrence_id.into_bytes()))
        }
        ActivitySubject::TranscriptWorkflow { workflow_id } => (6, Some(workflow_id.into_bytes())),
        ActivitySubject::Publication { publication_id } => (7, Some(publication_id.into_bytes())),
        ActivitySubject::Note { note_id } => (8, Some(note_id.into_bytes())),
        ActivitySubject::Memory { memory_id } => (9, Some(memory_id.into_bytes())),
        ActivitySubject::Clip { clip_id } => (10, Some(clip_id.into_bytes())),
        ActivitySubject::Operation { command_id } => (11, Some(command_id.into_bytes())),
    }
}

const fn effect_kind_code(value: ExternalEffectKind) -> u8 {
    match value {
        ExternalEffectKind::FeedNetwork => 1,
        ExternalEffectKind::Playback => 2,
        ExternalEffectKind::RecallProvider => 3,
        ExternalEffectKind::ChapterProvider => 4,
        ExternalEffectKind::Download => 5,
        ExternalEffectKind::Notification => 6,
        ExternalEffectKind::TranscriptProvider => 7,
        ExternalEffectKind::AgentProvider => 8,
        ExternalEffectKind::AgentApproval => 9,
        ExternalEffectKind::AgentCapability => 10,
        ExternalEffectKind::ScheduledAgentProvider => 11,
        ExternalEffectKind::CoreWake => 12,
        ExternalEffectKind::Filesystem => 13,
        ExternalEffectKind::Publication => 14,
    }
}

const fn domain_code(value: ActivityDomain) -> u8 {
    match value {
        ActivityDomain::LibraryFeed => 1,
        ActivityDomain::Playback => 2,
        ActivityDomain::Download => 3,
        ActivityDomain::Transcript => 4,
        ActivityDomain::Chapter => 5,
        ActivityDomain::RecallKnowledge => 6,
        ActivityDomain::ScheduledAgent => 7,
        ActivityDomain::AgentPublication => 8,
        ActivityDomain::UserArtifact => 9,
        ActivityDomain::Lifecycle => 10,
    }
}
