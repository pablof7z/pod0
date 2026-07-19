use pod0_application::{ApplicationCommand, RecallScope};
use sha2::{Digest, Sha256};

use crate::runtime_command_fingerprint_values::{
    hash_clip_source, hash_evidence_input, hash_note_target, hash_optional, hash_optional_speaker,
    hash_playback, hash_policy,
};

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
        ApplicationCommand::RebuildTranscriptEvidence { input, policy } => {
            hash_evidence_input(&mut hash, input, *policy);
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
        ApplicationCommand::CreateClip {
            clip_id,
            episode_id,
            podcast_id,
            start_milliseconds,
            end_milliseconds,
            caption,
            speaker_id,
            frozen_transcript_text,
            source,
        } => {
            hash.update(b"create-clip\0");
            hash.update(clip_id.into_bytes());
            hash.update(episode_id.into_bytes());
            hash.update(podcast_id.into_bytes());
            hash.update(start_milliseconds.to_be_bytes());
            hash.update(end_milliseconds.to_be_bytes());
            hash_optional(&mut hash, caption.as_deref());
            hash_optional_speaker(&mut hash, *speaker_id);
            hash.update(frozen_transcript_text.as_bytes());
            hash.update([0]);
            hash_clip_source(&mut hash, *source);
        }
        ApplicationCommand::UpdateClip {
            clip_id,
            expected_clip_revision,
            start_milliseconds,
            end_milliseconds,
            caption,
            speaker_id,
            frozen_transcript_text,
        } => {
            hash.update(b"update-clip\0");
            hash.update(clip_id.into_bytes());
            hash.update(expected_clip_revision.value.to_be_bytes());
            hash.update(start_milliseconds.to_be_bytes());
            hash.update(end_milliseconds.to_be_bytes());
            hash_optional(&mut hash, caption.as_deref());
            hash_optional_speaker(&mut hash, *speaker_id);
            hash.update(frozen_transcript_text.as_bytes());
            hash.update([0]);
        }
        ApplicationCommand::SetClipDeleted {
            clip_id,
            expected_clip_revision,
            deleted,
        } => {
            hash.update(b"delete-clip\0");
            hash.update(clip_id.into_bytes());
            hash.update(expected_clip_revision.value.to_be_bytes());
            hash.update([u8::from(*deleted)]);
        }
        ApplicationCommand::ClearClips {
            expected_collection_revision,
        } => {
            hash.update(b"clear-clips\0");
            hash.update(expected_collection_revision.value.to_be_bytes());
        }
        ApplicationCommand::CancelOperation { cancellation_id } => {
            hash.update(b"cancel\0");
            hash.update(cancellation_id.into_bytes());
        }
        ApplicationCommand::Unsupported { wire_code } => {
            hash.update(b"unsupported\0");
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hash_note_kind(hash: &mut Sha256, value: pod0_domain::NoteKind) {
    match value {
        pod0_domain::NoteKind::Free => hash.update([1]),
        pod0_domain::NoteKind::Reflection => hash.update([2]),
        pod0_domain::NoteKind::SystemEvent => hash.update([3]),
        pod0_domain::NoteKind::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}

fn hash_note_author(hash: &mut Sha256, value: pod0_domain::NoteAuthor) {
    match value {
        pod0_domain::NoteAuthor::User => hash.update([1]),
        pod0_domain::NoteAuthor::Agent => hash.update([2]),
        pod0_domain::NoteAuthor::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}
