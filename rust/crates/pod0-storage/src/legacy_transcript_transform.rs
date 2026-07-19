use std::collections::BTreeSet;

use pod0_domain::{
    ContentDigest, EpisodeId, PodcastId, SpeakerId, TranscriptArtifact, TranscriptArtifactInput,
    TranscriptArtifactSegmentInput, TranscriptArtifactSpeakerInput, TranscriptArtifactWordInput,
    TranscriptSource, UnixTimestampMilliseconds,
};

use crate::StorageError;
use crate::legacy_format::{finite_milliseconds, timestamp_milliseconds, uuid_bytes};
use crate::legacy_transcript_format::{
    RawTranscript, RawTranscriptSegment, RawTranscriptSpeaker, RawTranscriptWord,
};
use crate::transcript_import_digest::hex_digest;

pub(crate) fn transform_transcript(
    raw: RawTranscript,
    expected_episode_id: EpisodeId,
    podcast_id: PodcastId,
    file_digest: ContentDigest,
    index: u32,
) -> Result<TranscriptArtifact, StorageError> {
    let episode_id =
        EpisodeId::from_bytes(uuid_bytes(&raw.episode_id, "transcript episode", index)?);
    if episode_id != expected_episode_id {
        return Err(invalid(
            index,
            "selected transcript episode does not match selection",
        ));
    }
    let source = match raw.source.as_str() {
        "publisher" => TranscriptSource::Publisher,
        "scribeV1" => TranscriptSource::Scribe,
        "whisper" => TranscriptSource::Whisper,
        "onDevice" => TranscriptSource::OnDevice,
        "assemblyAI" => TranscriptSource::AssemblyAi,
        _ => return Err(invalid(index, "selected transcript source is unsupported")),
    };
    let speakers = transform_speakers(raw.speakers, index)?;
    let segments = transform_segments(raw.segments, index)?;
    let generated_at = timestamp_milliseconds(Some(&raw.generated_at), "transcript", index)?;
    TranscriptArtifact::seal(TranscriptArtifactInput {
        episode_id,
        podcast_id,
        source_revision: format!("selected-json-sha256:{}", hex_digest(file_digest)),
        source,
        provider: None,
        source_payload_digest: file_digest,
        language: raw.language,
        generated_at: UnixTimestampMilliseconds::new(generated_at),
        speakers,
        segments,
    })
    .map_err(|_| invalid(index, "selected transcript violates the canonical contract"))
}

fn transform_speakers(
    raw: Vec<RawTranscriptSpeaker>,
    index: u32,
) -> Result<Vec<TranscriptArtifactSpeakerInput>, StorageError> {
    let mut identities = BTreeSet::new();
    raw.into_iter()
        .map(|speaker| {
            let speaker_id =
                SpeakerId::from_bytes(uuid_bytes(&speaker.id, "transcript speaker", index)?);
            if !identities.insert(speaker_id) {
                return Err(invalid(index, "transcript speaker identity is duplicated"));
            }
            Ok(TranscriptArtifactSpeakerInput {
                speaker_id,
                label: speaker.label,
                display_name: speaker.display_name,
            })
        })
        .collect()
}

fn transform_segments(
    raw: Vec<RawTranscriptSegment>,
    index: u32,
) -> Result<Vec<TranscriptArtifactSegmentInput>, StorageError> {
    let mut identities = BTreeSet::new();
    raw.into_iter()
        .map(|segment| {
            let identity = uuid_bytes(&segment.id, "transcript segment", index)?;
            if !identities.insert(identity) {
                return Err(invalid(index, "transcript segment identity is duplicated"));
            }
            Ok(TranscriptArtifactSegmentInput {
                text: segment.text,
                start_milliseconds: milliseconds(segment.start, index)?,
                end_milliseconds: milliseconds(segment.end, index)?,
                speaker_id: segment
                    .speaker_id
                    .as_deref()
                    .map(|value| {
                        uuid_bytes(value, "transcript segment speaker", index)
                            .map(SpeakerId::from_bytes)
                    })
                    .transpose()?,
                words: segment
                    .words
                    .unwrap_or_default()
                    .into_iter()
                    .map(|word| transform_word(word, index))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect()
}

fn transform_word(
    raw: RawTranscriptWord,
    index: u32,
) -> Result<TranscriptArtifactWordInput, StorageError> {
    Ok(TranscriptArtifactWordInput {
        text: raw.text,
        start_milliseconds: milliseconds(raw.start, index)?,
        end_milliseconds: milliseconds(raw.end, index)?,
    })
}

fn milliseconds(value: f64, index: u32) -> Result<u64, StorageError> {
    u64::try_from(finite_milliseconds(value, "transcript", index)?)
        .map_err(|_| invalid(index, "transcript timestamp is outside supported range"))
}

fn invalid(index: u32, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "transcript",
        index,
        detail,
    }
}
