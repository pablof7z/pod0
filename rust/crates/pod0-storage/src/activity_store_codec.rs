use pod0_application::{
    ActivityActor, ActivityFact, ActivityFactDraft, ActivityOrigin, ActivitySubject,
};

pub(super) fn verify_stored_draft(
    row: &rusqlite::Row<'_>,
    draft: &ActivityFactDraft,
) -> rusqlite::Result<()> {
    let (stored_subject_code, stored_subject_id) = subject(draft.subject);
    let matches = row.get::<_, Vec<u8>>(3)? == draft.activity_id.into_bytes()
        && row.get::<_, Vec<u8>>(4)? == draft.transaction_id.into_bytes()
        && row.get::<_, Vec<u8>>(5)? == draft.correlation_id.into_bytes()
        && row.get::<_, Option<Vec<u8>>>(6)?
            == draft
                .caused_by_activity_id
                .map(|value| value.into_bytes().to_vec())
        && row.get::<_, Option<Vec<u8>>>(7)?
            == draft.command_id.map(|value| value.into_bytes().to_vec())
        && row.get::<_, Option<Vec<u8>>>(8)?
            == draft
                .host_request_id
                .map(|value| value.into_bytes().to_vec())
        && row.get::<_, u8>(9)? == actor_code(draft.actor)
        && row.get::<_, u8>(10)? == origin_code(draft.origin)
        && row.get::<_, u8>(11)? == stored_subject_code
        && row.get::<_, Option<Vec<u8>>>(12)? == stored_subject_id.map(|value| value.to_vec())
        && row.get::<_, Option<Vec<u8>>>(13)?
            == draft.episode_id.map(|value| value.into_bytes().to_vec())
        && row.get::<_, u8>(14)? == fact_code(draft.fact);
    matches.then_some(()).ok_or(rusqlite::Error::InvalidQuery)
}

pub(super) const fn actor_code(value: ActivityActor) -> u8 {
    match value {
        ActivityActor::User => 1,
        ActivityActor::System => 2,
        ActivityActor::Agent => 3,
        ActivityActor::Recovery => 4,
        ActivityActor::Migration => 5,
        ActivityActor::Unsupported { .. } => 255,
    }
}

pub(super) const fn origin_code(value: ActivityOrigin) -> u8 {
    match value {
        ActivityOrigin::UserInterface => 1,
        ActivityOrigin::AutomaticPolicy => 2,
        ActivityOrigin::Playback => 3,
        ActivityOrigin::AgentTool => 4,
        ActivityOrigin::ScheduledWork => 5,
        ActivityOrigin::HostObservation => 6,
        ActivityOrigin::Recovery => 7,
        ActivityOrigin::Migration => 8,
        ActivityOrigin::InternalCommand => 9,
        ActivityOrigin::Unsupported { .. } => 255,
    }
}

pub(super) fn subject(value: ActivitySubject) -> (u8, Option<[u8; 16]>) {
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
        ActivitySubject::SpeakerEntity { speaker_entity_id } => {
            (12, Some(speaker_entity_id.into_bytes()))
        }
        ActivitySubject::TranscriptArtifact { artifact_id } => (13, Some(artifact_id.into_bytes())),
    }
}

pub(super) const fn fact_code(value: ActivityFact) -> u8 {
    match value {
        ActivityFact::RequestDisposition { .. } => 1,
        ActivityFact::DomainTransition { .. } => 2,
        ActivityFact::PlaybackCheckpoint { .. } => 3,
        ActivityFact::EffectAuthorized { .. } => 4,
        ActivityFact::EffectObserved { .. } => 5,
        ActivityFact::InternalCommandAuthorized { .. } => 6,
        ActivityFact::RecoveryTransition { .. } => 7,
        ActivityFact::AuthorityCutover { .. } => 8,
    }
}
