use pod0_application::{ApplicationCommand, RecallScope};
use sha2::{Digest, Sha256};

use crate::runtime_artifact_command_fingerprint::{hash_chapter_commit, hash_transcript_commit};
use crate::runtime_clip_command_fingerprint::hash_clip_command;
use crate::runtime_command_fingerprint_values::{
    finish_command_hash, hash_command_tail, hash_evidence_input, hash_note_author, hash_note_kind,
    hash_note_target, hash_optional, hash_policy,
};
use crate::runtime_download_command_fingerprint::hash_download_command;
use crate::runtime_playback_fingerprint::hash_playback;
use crate::runtime_scheduled_agent_command_fingerprint::hash_scheduled_agent_command;
use crate::runtime_transcript_workflow_fingerprint::hash_transcript_workflow_command;

pub(super) fn command_fingerprint(command: &ApplicationCommand) -> String {
    let mut hash = Sha256::new();
    match command {
        ApplicationCommand::SubscribeToFeed { feed_url } => {
            hash.update(b"subscribe\0");
            hash.update(feed_url.as_bytes());
        }
        ApplicationCommand::EnsurePodcast { feed_url } => {
            hash.update(b"ensure\0");
            hash.update(feed_url.as_bytes());
        }
        ApplicationCommand::RefreshPodcast { podcast_id } => {
            hash.update(b"refresh\0");
            hash.update(podcast_id.into_bytes());
        }
        ApplicationCommand::HydratePodcastMetadata { podcast_id } => {
            hash.update(b"hydrate-metadata\0");
            hash.update(podcast_id.into_bytes());
        }
        ApplicationCommand::UpsertSyntheticPodcast { podcast } => {
            hash.update(b"synthetic-podcast\0");
            match podcast.podcast_id {
                Some(id) => {
                    hash.update([1]);
                    hash.update(id.into_bytes());
                }
                None => hash.update([0]),
            }
            hash.update(podcast.title.as_bytes());
            hash.update([0]);
            hash.update(podcast.author.as_bytes());
            hash_optional(&mut hash, podcast.image_url.as_deref());
            hash.update(podcast.description.as_bytes());
            hash_optional(&mut hash, podcast.language.as_deref());
            hash.update((podcast.categories.len() as u64).to_be_bytes());
            for category in &podcast.categories {
                hash.update(category.as_bytes());
                hash.update([0]);
            }
        }
        ApplicationCommand::UpsertExternalEpisode { episode } => {
            hash.update(b"external-episode\0");
            hash.update(episode.podcast_id.into_bytes());
            hash_optional(&mut hash, episode.feed_url.as_deref());
            hash.update(episode.podcast_title.as_bytes());
            hash.update([0]);
            hash.update(episode.audio_url.as_bytes());
            hash.update([0]);
            hash.update(episode.title.as_bytes());
            hash.update([0]);
            hash.update(episode.description.as_bytes());
            hash.update(episode.published_at.value.to_be_bytes());
            hash_optional(&mut hash, episode.enclosure_mime_type.as_deref());
            hash_optional(&mut hash, episode.image_url.as_deref());
            hash.update(
                episode
                    .duration_milliseconds
                    .unwrap_or(u64::MAX)
                    .to_be_bytes(),
            );
        }
        ApplicationCommand::Unsubscribe { podcast_id } => {
            hash.update(b"unsubscribe\0");
            hash.update(podcast_id.into_bytes());
        }
        ApplicationCommand::SetSubscriptionNotifications {
            podcast_id,
            enabled,
        } => {
            hash.update(b"notifications\0");
            hash.update(podcast_id.into_bytes());
            hash.update([u8::from(*enabled)]);
        }
        ApplicationCommand::SetSubscriptionAutoDownload { podcast_id, policy } => {
            hash.update(b"auto-download\0");
            hash.update(podcast_id.into_bytes());
            hash_policy(&mut hash, policy);
        }
        ApplicationCommand::SetEpisodeStarred {
            episode_id,
            starred,
        } => {
            hash.update(b"episode-starred\0");
            hash.update(episode_id.into_bytes());
            hash.update([u8::from(*starred)]);
        }
        ApplicationCommand::RequestEpisodeDownload { .. }
        | ApplicationCommand::CancelEpisodeDownload { .. }
        | ApplicationCommand::RemoveEpisodeDownload { .. }
        | ApplicationCommand::ObserveDownloadEnvironment { .. } => {
            hash_download_command(&mut hash, command)
        }
        ApplicationCommand::ReportAutomaticDownloadCandidates {
            podcast_id,
            episode_ids,
        } => {
            hash.update(b"automatic-download-candidates\0");
            hash.update(podcast_id.into_bytes());
            hash.update((episode_ids.len() as u64).to_be_bytes());
            for episode_id in episode_ids {
                hash.update(episode_id.into_bytes());
            }
        }
        ApplicationCommand::ResetListeningData => hash.update(b"reset-listening\0"),
        ApplicationCommand::RequestPlayback { episode_id } => {
            hash.update(b"play\0");
            hash.update(episode_id.into_bytes());
        }
        ApplicationCommand::Playback { command } => hash_playback(&mut hash, command),
        ApplicationCommand::RecallQuery { query } => {
            hash.update(b"recall-query\0");
            hash.update(query.query_id.into_bytes());
            hash.update(query.text.as_bytes());
            hash.update([0]);
            hash.update(query.limit.to_be_bytes());
            match query.scope {
                RecallScope::Library => hash.update([1]),
                RecallScope::Podcast { podcast_id } => {
                    hash.update([2]);
                    hash.update(podcast_id.into_bytes());
                }
                RecallScope::Episode { episode_id } => {
                    hash.update([3]);
                    hash.update(episode_id.into_bytes());
                }
                RecallScope::Unsupported { wire_code } => {
                    hash.update([255]);
                    hash.update(wire_code.to_be_bytes());
                }
            }
        }
        ApplicationCommand::ImportLegacyRecallConfiguration {
            configuration,
            source_generation,
        } => {
            hash.update(b"import-legacy-recall-configuration\0");
            hash.update(configuration.stored_embedding_model_id.as_bytes());
            hash.update([0, u8::from(configuration.reranker_enabled)]);
            hash.update(source_generation.into_bytes());
        }
        ApplicationCommand::SetRecallConfiguration {
            expected_configuration_revision,
            configuration,
        } => {
            hash.update(b"set-recall-configuration\0");
            hash.update(expected_configuration_revision.value.to_be_bytes());
            hash.update(configuration.stored_embedding_model_id.as_bytes());
            hash.update([0, u8::from(configuration.reranker_enabled)]);
        }
        ApplicationCommand::RebuildTranscriptEvidence { input, policy } => {
            hash_evidence_input(&mut hash, input, *policy);
        }
        ApplicationCommand::CommitRecallIndexCutover => {
            hash.update(b"commit-recall-index-cutover\0");
        }
        ApplicationCommand::CommitTranscript {
            expected_selection_revision,
            artifact,
        } => {
            hash_transcript_commit(&mut hash, *expected_selection_revision, artifact);
        }
        ApplicationCommand::EnsureTranscriptWorkflow { .. }
        | ApplicationCommand::RetryTranscriptWorkflow { .. }
        | ApplicationCommand::CancelTranscriptWorkflow { .. } => {
            hash_transcript_workflow_command(&mut hash, command)
        }
        ApplicationCommand::EnsureScheduledTask { .. }
        | ApplicationCommand::UpdateScheduledTask { .. }
        | ApplicationCommand::RemoveScheduledTask { .. }
        | ApplicationCommand::ReconcileScheduledRuns
        | ApplicationCommand::CancelScheduledRun { .. } => {
            hash_scheduled_agent_command(&mut hash, command)
        }
        ApplicationCommand::CommitChapter {
            expected_selection_revision,
            artifact,
        } => {
            hash_chapter_commit(&mut hash, *expected_selection_revision, artifact);
        }
        ApplicationCommand::EnsurePublisherChapters { episode_id } => {
            hash.update(b"ensure-publisher-chapters\0");
            hash.update(episode_id.into_bytes());
        }
        ApplicationCommand::RetryPublisherChapters {
            episode_id,
            expected_workflow_revision,
        } => {
            hash.update(b"retry-publisher-chapters\0");
            hash.update(episode_id.into_bytes());
            hash.update(expected_workflow_revision.value.to_be_bytes());
        }
        ApplicationCommand::CancelPublisherChapters {
            episode_id,
            expected_workflow_revision,
        } => {
            hash.update(b"cancel-publisher-chapters\0");
            hash.update(episode_id.into_bytes());
            hash.update(expected_workflow_revision.value.to_be_bytes());
        }
        ApplicationCommand::EnsureModelChapters {
            episode_id,
            configured_model,
        } => {
            hash.update(b"ensure-model-chapters\0");
            hash.update(episode_id.into_bytes());
            hash.update(configured_model.as_bytes());
            hash.update([0]);
        }
        ApplicationCommand::RetryModelChapters {
            episode_id,
            configured_model,
            expected_workflow_revision,
        } => {
            hash.update(b"retry-model-chapters\0");
            hash.update(episode_id.into_bytes());
            hash.update(configured_model.as_bytes());
            hash.update([0]);
            hash.update(expected_workflow_revision.value.to_be_bytes());
        }
        ApplicationCommand::CancelModelChapters {
            episode_id,
            expected_workflow_revision,
        } => {
            hash.update(b"cancel-model-chapters\0");
            hash.update(episode_id.into_bytes());
            hash.update(expected_workflow_revision.value.to_be_bytes());
        }
        ApplicationCommand::CreateNote {
            text,
            kind,
            author,
            target,
        } => {
            hash.update(b"create-note\0");
            hash.update(text.as_bytes());
            hash.update([0]);
            hash_note_kind(&mut hash, *kind);
            hash_note_author(&mut hash, *author);
            hash_note_target(&mut hash, *target);
        }
        ApplicationCommand::UpdateNote {
            note_id,
            expected_note_revision,
            text,
            kind,
            target,
        } => {
            hash.update(b"update-note\0");
            hash.update(note_id.into_bytes());
            hash.update(expected_note_revision.value.to_be_bytes());
            hash.update(text.as_bytes());
            hash.update([0]);
            hash_note_kind(&mut hash, *kind);
            hash_note_target(&mut hash, *target);
        }
        ApplicationCommand::SetNoteDeleted {
            note_id,
            expected_note_revision,
            deleted,
        } => {
            hash.update(b"delete-note\0");
            hash.update(note_id.into_bytes());
            hash.update(expected_note_revision.value.to_be_bytes());
            hash.update([u8::from(*deleted)]);
        }
        ApplicationCommand::ClearNotes {
            expected_collection_revision,
        } => {
            hash.update(b"clear-notes\0");
            hash.update(expected_collection_revision.value.to_be_bytes());
        }
        ApplicationCommand::CreateClip { .. }
        | ApplicationCommand::UpdateClip { .. }
        | ApplicationCommand::SetClipDeleted { .. }
        | ApplicationCommand::ClearClips { .. } => hash_clip_command(&mut hash, command),
        ApplicationCommand::CancelOperation { .. } | ApplicationCommand::Unsupported { .. } => {
            hash_command_tail(&mut hash, command)
        }
    }
    finish_command_hash(hash)
}
