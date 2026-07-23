use pod0_domain::{
    ArtifactReference, CompletionCause, CompletionStatus, DownloadArtifactStatus,
    EpisodeFeedMetadata, EpisodeId, EpisodeListeningState, EpisodeRecord, PodcastId,
    PodcastPersonRecord, PodcastSoundBiteRecord, PublisherTranscriptFormat,
    PublisherTranscriptReference, TranscriptArtifactStatus, TranscriptSource,
    UnixTimestampMilliseconds,
};
use serde_json::Value;

use crate::StorageError;
use crate::legacy_format::{
    RawEpisode, enum_variant, finite_milliseconds, timestamp_milliseconds, unknown_wire_code,
    uuid_bytes,
};

pub(crate) fn episodes(payloads: &[Vec<u8>]) -> Result<Vec<EpisodeRecord>, StorageError> {
    payloads
        .iter()
        .enumerate()
        .map(|(index, payload)| episode(payload, u32::try_from(index).unwrap_or(u32::MAX)))
        .collect()
}

fn episode(payload: &[u8], index: u32) -> Result<EpisodeRecord, StorageError> {
    let raw: RawEpisode =
        serde_json::from_slice(payload).map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "episode",
            index,
            detail: "episode payload is not recognized JSON",
        })?;
    let episode_bytes = uuid_bytes(&raw.id, "episode", index)?;
    let parent = raw
        .podcast_id
        .as_ref()
        .or(raw.legacy_subscription_id.as_ref())
        .ok_or(StorageError::InvalidLegacyRecord {
            entity: "episode",
            index,
            detail: "episode has no podcast identity",
        })?;
    let position = finite_milliseconds(raw.playback_position, "episode", index)?;
    let position = u64::try_from(position).map_err(|_| invalid_episode(index))?;
    let completion = if raw.played {
        CompletionStatus::Completed {
            cause: CompletionCause::LegacyPlayedFlag,
        }
    } else {
        CompletionStatus::InProgress
    };
    let feed_metadata = feed_metadata(&raw, index)?;
    Ok(EpisodeRecord {
        episode_id: EpisodeId::from_bytes(episode_bytes),
        podcast_id: PodcastId::from_bytes(uuid_bytes(parent, "episode", index)?),
        publisher_guid: raw.guid,
        title: raw.title,
        description: raw.description,
        published_at: UnixTimestampMilliseconds::new(timestamp_milliseconds(
            raw.published_at.as_ref(),
            "episode",
            index,
        )?),
        duration_milliseconds: raw
            .duration
            .map(|value| finite_milliseconds(value, "episode", index))
            .transpose()?
            .map(|value| u64::try_from(value).map_err(|_| invalid_episode(index)))
            .transpose()?,
        enclosure_url: raw.enclosure_url,
        enclosure_mime_type: raw.enclosure_mime_type,
        image_url: raw.image_url,
        feed_metadata,
        listening: EpisodeListeningState {
            resume_position_milliseconds: position,
            completion,
        },
        is_starred: raw.is_starred,
        download: download(raw.download_state.as_ref(), &episode_bytes, index)?,
        transcript: transcript(raw.transcript_state.as_ref(), &episode_bytes, index)?,
        generated_audio: None,
    })
}

fn feed_metadata(raw: &RawEpisode, index: u32) -> Result<EpisodeFeedMetadata, StorageError> {
    let publisher_transcript =
        raw.publisher_transcript_url
            .as_ref()
            .map(|url| PublisherTranscriptReference {
                url: url.clone(),
                media_type: None,
                format: transcript_format(raw.publisher_transcript_type.as_deref()),
            });
    let persons = raw
        .persons
        .iter()
        .filter(|person| !person.name.trim().is_empty())
        .map(|person| PodcastPersonRecord {
            name: person.name.clone(),
            role: person.role.clone(),
            group: person.group.clone(),
            image_url: person.image_url.clone(),
            link_url: person.link_url.clone(),
        })
        .collect();
    let sound_bites = raw
        .sound_bites
        .iter()
        .map(|sound_bite| {
            Ok(PodcastSoundBiteRecord {
                start_milliseconds: legacy_seconds(sound_bite.start_time, index)?,
                duration_milliseconds: legacy_seconds(sound_bite.duration, index)?,
                title: sound_bite.title.clone(),
            })
        })
        .collect::<Result<_, StorageError>>()?;
    Ok(EpisodeFeedMetadata {
        publisher_transcript,
        chapters_url: raw.chapters_url.clone(),
        persons,
        sound_bites,
    })
}

fn legacy_seconds(value: f64, index: u32) -> Result<u64, StorageError> {
    u64::try_from(finite_milliseconds(value, "episode", index)?).map_err(|_| invalid_episode(index))
}

fn transcript_format(value: Option<&str>) -> PublisherTranscriptFormat {
    match value {
        Some("json") => PublisherTranscriptFormat::Json,
        Some("vtt") => PublisherTranscriptFormat::WebVtt,
        Some("srt") => PublisherTranscriptFormat::SubRip,
        Some("html") => PublisherTranscriptFormat::Html,
        Some("text") => PublisherTranscriptFormat::PlainText,
        Some(_) => PublisherTranscriptFormat::Unknown,
        None => PublisherTranscriptFormat::Unknown,
    }
}

fn download(
    raw: Option<&Value>,
    episode_id: &[u8; 16],
    index: u32,
) -> Result<DownloadArtifactStatus, StorageError> {
    let Some(raw) = raw else {
        return Ok(DownloadArtifactStatus::Unavailable);
    };
    let (name, payload) = enum_variant(raw).ok_or(invalid_episode(index))?;
    match name {
        "notDownloaded" => Ok(DownloadArtifactStatus::Unavailable),
        "downloaded" => {
            let byte_count = payload
                .get("byteCount")
                .and_then(Value::as_i64)
                .and_then(|value| u64::try_from(value).ok())
                .ok_or(invalid_episode(index))?;
            Ok(DownloadArtifactStatus::Available {
                reference: artifact("download", episode_id),
                byte_count,
            })
        }
        other => Ok(DownloadArtifactStatus::Unsupported {
            wire_code: unknown_wire_code(other),
        }),
    }
}

fn transcript(
    raw: Option<&Value>,
    episode_id: &[u8; 16],
    index: u32,
) -> Result<TranscriptArtifactStatus, StorageError> {
    let Some(raw) = raw else {
        return Ok(TranscriptArtifactStatus::Unavailable);
    };
    let (name, payload) = enum_variant(raw).ok_or(invalid_episode(index))?;
    match name {
        "none" => Ok(TranscriptArtifactStatus::Unavailable),
        "ready" => {
            let source = payload
                .get("source")
                .and_then(Value::as_str)
                .ok_or(invalid_episode(index))?;
            Ok(TranscriptArtifactStatus::Available {
                reference: artifact("transcript", episode_id),
                source: transcript_source(source),
            })
        }
        other => Ok(TranscriptArtifactStatus::Unsupported {
            wire_code: unknown_wire_code(other),
        }),
    }
}

fn transcript_source(value: &str) -> TranscriptSource {
    match value {
        "publisher" => TranscriptSource::Publisher,
        "scribe" => TranscriptSource::Scribe,
        "whisper" => TranscriptSource::Whisper,
        "onDevice" => TranscriptSource::OnDevice,
        "assemblyAI" => TranscriptSource::AssemblyAi,
        "other" => TranscriptSource::Other,
        value => TranscriptSource::Unsupported {
            wire_code: unknown_wire_code(value),
        },
    }
}

fn artifact(kind: &str, id: &[u8; 16]) -> ArtifactReference {
    ArtifactReference {
        schema_version: 1,
        opaque_key: format!("legacy-{kind}:{}:v1", hex(id)),
    }
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid_episode(index: u32) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "episode",
        index,
        detail: "episode state is malformed",
    }
}
