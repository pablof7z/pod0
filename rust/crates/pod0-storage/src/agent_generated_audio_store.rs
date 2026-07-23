use pod0_application::{
    AgentToolAction, AgentTurnStage, AgentTurnState, MAX_AGENT_GENERATED_AUDIO_BYTES,
    agent_generated_artifact_id, agent_generated_episode_id, agent_generated_script_digest,
    default_agent_generated_podcast_id, normalize_media_url,
};
use pod0_domain::{
    DownloadArtifactStatus, EpisodeFeedMetadata, EpisodeId, EpisodeListeningState, EpisodeRecord,
    GeneratedAudioArtifactProvenance, PodcastId, PodcastKind, PodcastRecord,
    TranscriptArtifactStatus,
};
use rusqlite::{OptionalExtension, params};

use crate::agent_store::{command_receipt, persist};
use crate::library_store::{command_was_applied, finish_command};
use crate::library_store_feed::{upsert_episode, upsert_podcast};
use crate::{AgentAuditKind, AgentCommandContext, AgentMutationOutcome, AgentStore, StorageError};

#[path = "schema_agent_generated_audio.rs"]
pub(crate) mod schema;
#[cfg(test)]
#[path = "agent_generated_audio_store_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentGeneratedAudioCommitInput {
    pub podcast_id: PodcastId,
    pub episode_id: EpisodeId,
    pub title: String,
    pub audio_url: String,
    pub media_type: String,
    pub duration_milliseconds: Option<u64>,
    pub provenance: GeneratedAudioArtifactProvenance,
}

impl AgentStore {
    pub fn commit_generated_audio(
        &self,
        context: AgentCommandContext,
        expected_revision: pod0_domain::StateRevision,
        state: &AgentTurnState,
        input: &AgentGeneratedAudioCommitInput,
    ) -> Result<AgentMutationOutcome, StorageError> {
        self.write(|transaction| {
            let projection = state.projection();
            if let Some(duplicate) = command_receipt(transaction, context, projection.turn_id)? {
                verify_committed_artifact(transaction, &duplicate, input)?;
                return Ok(AgentMutationOutcome::Duplicate(duplicate));
            }
            validate_input(state, expected_revision, input)?;
            ensure_generated_podcast(transaction, input.podcast_id, context.observed_at)?;
            upsert_episode(transaction, &episode_record(input, context.observed_at))?;
            insert_artifact(transaction, input)?;
            let library_fingerprint = library_fingerprint(context.command_fingerprint);
            if command_was_applied(transaction, context.command_id, &library_fingerprint)?.is_some()
            {
                return Err(StorageError::AgentCommandConflict);
            }
            finish_command(
                transaction,
                context.command_id,
                &library_fingerprint,
                context.observed_at.value,
            )?;
            persist(
                transaction,
                context,
                Some(expected_revision),
                AgentAuditKind::ActionObserved,
                state,
            )
        })
    }
}

fn validate_input(
    state: &AgentTurnState,
    expected_revision: pod0_domain::StateRevision,
    input: &AgentGeneratedAudioCommitInput,
) -> Result<(), StorageError> {
    let projection = state.projection();
    let proposal = projection
        .proposal
        .as_ref()
        .ok_or(StorageError::InvalidAgentState)?;
    let commit = projection
        .commit
        .as_ref()
        .ok_or(StorageError::InvalidAgentState)?;
    let AgentToolAction::GenerateTtsEpisode {
        podcast_id,
        title,
        script,
        voice_id,
    } = &proposal.action
    else {
        return Err(StorageError::InvalidAgentState);
    };
    let resolved_podcast_id = podcast_id.unwrap_or_else(default_agent_generated_podcast_id);
    if projection.stage != AgentTurnStage::AwaitingModel
        || projection.revision.value <= expected_revision.value
        || input.podcast_id != resolved_podcast_id
        || input.episode_id != agent_generated_episode_id(input.podcast_id, &input.audio_url)
        || input.title != *title
        || input.media_type != "audio/mpeg"
        || normalize_media_url(&input.audio_url).as_deref() != Some(input.audio_url.as_str())
        || !input.audio_url.starts_with("file://")
        || !(1..=MAX_AGENT_GENERATED_AUDIO_BYTES).contains(&input.provenance.media_byte_count)
        || input.provenance.conversation_id != projection.conversation_id
        || input.provenance.turn_id != projection.turn_id
        || input.provenance.proposal_id != proposal.proposal_id
        || input.provenance.artifact_id
            != agent_generated_artifact_id(proposal.proposal_id, proposal.proposal_digest)
        || input.provenance.commit_id != commit.commit_id
        || commit.artifact_id != Some(input.provenance.artifact_id)
        || input.provenance.script_content_digest != agent_generated_script_digest(script)
        || input.provenance.voice_id != *voice_id
        || input.provenance.model_reference != state.model_reference()
        || input.provenance.committed_at != commit.committed_at
    {
        return Err(StorageError::InvalidAgentState);
    }
    Ok(())
}

fn ensure_generated_podcast(
    transaction: &rusqlite::Transaction<'_>,
    podcast_id: PodcastId,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> Result<(), StorageError> {
    let existing_kind: Option<i64> = transaction
        .query_row(
            "SELECT kind_code FROM pod0_podcasts WHERE podcast_id=?1",
            [podcast_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("find generated audio podcast", error))?;
    match existing_kind {
        Some(2) => Ok(()),
        Some(_) => Err(StorageError::AgentTurnConflict),
        None if podcast_id == default_agent_generated_podcast_id() => upsert_podcast(
            transaction,
            &PodcastRecord {
                podcast_id,
                kind: PodcastKind::Synthetic,
                feed_identity: None,
                title: "Agent Generated".into(),
                author: "Podcast Agent".into(),
                image_url: None,
                description: "Episodes generated by your AI agent.".into(),
                language: None,
                categories: Vec::new(),
                discovered_at: observed_at,
                title_is_placeholder: false,
                last_refreshed_at: None,
                etag: None,
                last_modified: None,
            },
        ),
        None => Err(StorageError::AgentTurnConflict),
    }
}

fn episode_record(
    input: &AgentGeneratedAudioCommitInput,
    observed_at: pod0_domain::UnixTimestampMilliseconds,
) -> EpisodeRecord {
    EpisodeRecord {
        episode_id: input.episode_id,
        podcast_id: input.podcast_id,
        publisher_guid: input.audio_url.clone(),
        title: input.title.clone(),
        description: "Generated by Pod0 Agent.".into(),
        published_at: observed_at,
        duration_milliseconds: input.duration_milliseconds,
        enclosure_url: input.audio_url.clone(),
        enclosure_mime_type: Some(input.media_type.clone()),
        image_url: None,
        feed_metadata: EpisodeFeedMetadata::default(),
        listening: EpisodeListeningState {
            resume_position_milliseconds: 0,
            completion: pod0_domain::CompletionStatus::InProgress,
        },
        is_starred: false,
        download: DownloadArtifactStatus::Unavailable,
        transcript: TranscriptArtifactStatus::Unavailable,
        generated_audio: Some(input.provenance.clone()),
    }
}

fn insert_artifact(
    transaction: &rusqlite::Transaction<'_>,
    input: &AgentGeneratedAudioCommitInput,
) -> Result<(), StorageError> {
    let provenance = &input.provenance;
    transaction
        .execute(
            "INSERT INTO pod0_agent_generated_audio_artifacts(artifact_id,episode_id,podcast_id,\
             conversation_id,turn_id,proposal_id,commit_id,media_url,media_type,media_byte_count,\
             media_content_digest,script_content_digest,voice_id,model_reference,committed_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                provenance.artifact_id.into_bytes().as_slice(),
                input.episode_id.into_bytes().as_slice(),
                input.podcast_id.into_bytes().as_slice(),
                provenance.conversation_id.into_bytes().as_slice(),
                provenance.turn_id.into_bytes().as_slice(),
                provenance.proposal_id.into_bytes().as_slice(),
                provenance.commit_id.into_bytes().as_slice(),
                input.audio_url,
                input.media_type,
                i64::try_from(provenance.media_byte_count)
                    .map_err(|_| StorageError::InvalidAgentState)?,
                provenance.media_content_digest.into_bytes().as_slice(),
                provenance.script_content_digest.into_bytes().as_slice(),
                provenance.voice_id,
                provenance.model_reference,
                provenance.committed_at.value,
            ],
        )
        .map_err(|error| StorageError::sqlite("commit agent generated audio artifact", error))?;
    Ok(())
}

fn verify_committed_artifact(
    transaction: &rusqlite::Transaction<'_>,
    state: &AgentTurnState,
    input: &AgentGeneratedAudioCommitInput,
) -> Result<(), StorageError> {
    let stored: Option<(Vec<u8>, Vec<u8>)> = transaction
        .query_row(
            "SELECT episode_id,media_content_digest FROM pod0_agent_generated_audio_artifacts \
             WHERE artifact_id=?1",
            [input.provenance.artifact_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("verify agent generated audio replay", error))?;
    let expected_commit = state
        .projection()
        .commit
        .and_then(|commit| commit.artifact_id)
        == Some(input.provenance.artifact_id);
    if stored
        != Some((
            input.episode_id.into_bytes().to_vec(),
            input.provenance.media_content_digest.into_bytes().to_vec(),
        ))
        || !expected_commit
    {
        return Err(StorageError::CorruptSchema {
            detail: "agent generated audio replay disagrees with committed state",
        });
    }
    Ok(())
}

fn library_fingerprint(fingerprint: [u8; 32]) -> String {
    fingerprint
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
