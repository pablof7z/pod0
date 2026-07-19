use pod0_domain::{TranscriptArtifact, TranscriptSource};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::StorageError;
use crate::transcript_import_digest::{digest_bytes, hex_digest};

pub(crate) const ROLLBACK_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct RollbackManifest {
    pub(crate) format_version: u32,
    pub(crate) core_schema_version: u32,
    pub(crate) transcript_revision: u64,
    pub(crate) entries: Vec<RollbackManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct RollbackManifestEntry {
    pub(crate) episode_id: String,
    pub(crate) podcast_id: String,
    pub(crate) artifact_id: String,
    pub(crate) transcript_version_id: String,
    pub(crate) transcript_content_digest: String,
    pub(crate) artifact_integrity_digest: String,
    pub(crate) exported_file_digest: String,
    pub(crate) source_revision: String,
    pub(crate) is_selected: bool,
    pub(crate) relative_path: String,
}

#[derive(Deserialize, Serialize)]
struct LegacyTranscript {
    id: String,
    #[serde(rename = "episodeID")]
    episode_id: String,
    language: String,
    source: String,
    segments: Vec<LegacySegment>,
    speakers: Vec<LegacySpeaker>,
    #[serde(rename = "generatedAt")]
    generated_at: String,
}

#[derive(Deserialize, Serialize)]
struct LegacySegment {
    id: String,
    start: f64,
    end: f64,
    #[serde(rename = "speakerID")]
    speaker_id: Option<String>,
    text: String,
    words: Option<Vec<LegacyWord>>,
}

#[derive(Deserialize, Serialize)]
struct LegacyWord {
    start: f64,
    end: f64,
    text: String,
}

#[derive(Deserialize, Serialize)]
struct LegacySpeaker {
    id: String,
    label: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

pub(crate) fn legacy_transcript_bytes(
    artifact: &TranscriptArtifact,
) -> Result<Vec<u8>, StorageError> {
    artifact
        .verify_integrity()
        .map_err(|_| StorageError::InvalidTranscriptArtifact)?;
    let transcript = LegacyTranscript {
        id: uuid_string(artifact.transcript_version_id.into_bytes()),
        episode_id: uuid_string(artifact.episode_id.into_bytes()),
        language: artifact.language.clone(),
        source: legacy_source(artifact.provenance.source)?.to_owned(),
        segments: artifact
            .segments
            .iter()
            .map(|segment| LegacySegment {
                id: uuid_string(segment.segment_id.into_bytes()),
                start: seconds(segment.start_milliseconds),
                end: seconds(segment.end_milliseconds),
                speaker_id: segment.speaker_id.map(|id| uuid_string(id.into_bytes())),
                text: segment.text.clone(),
                words: (!segment.words.is_empty()).then(|| {
                    segment
                        .words
                        .iter()
                        .map(|word| LegacyWord {
                            start: seconds(word.start_milliseconds),
                            end: seconds(word.end_milliseconds),
                            text: word.text.clone(),
                        })
                        .collect()
                }),
            })
            .collect(),
        speakers: artifact
            .speakers
            .iter()
            .map(|speaker| LegacySpeaker {
                id: uuid_string(speaker.speaker_id.into_bytes()),
                label: speaker.label.clone(),
                display_name: speaker.display_name.clone(),
            })
            .collect(),
        generated_at: timestamp(artifact.generated_at.value())?,
    };
    serde_json::to_vec(&transcript).map_err(|_| StorageError::InvalidTranscriptArtifact)
}

pub(crate) fn manifest_entry(
    artifact: &TranscriptArtifact,
    exported_bytes: &[u8],
    is_selected: bool,
    relative_path: String,
) -> RollbackManifestEntry {
    RollbackManifestEntry {
        episode_id: uuid_string(artifact.episode_id.into_bytes()),
        podcast_id: uuid_string(artifact.podcast_id.into_bytes()),
        artifact_id: hex_id(artifact.artifact_id.into_bytes()),
        transcript_version_id: hex_id(artifact.transcript_version_id.into_bytes()),
        transcript_content_digest: hex_digest(artifact.content_digest),
        artifact_integrity_digest: hex_digest(artifact.integrity_digest),
        exported_file_digest: hex_digest(digest_bytes(exported_bytes)),
        source_revision: artifact.source_revision.clone(),
        is_selected,
        relative_path,
    }
}

pub(crate) fn legacy_source(source: TranscriptSource) -> Result<&'static str, StorageError> {
    match source {
        TranscriptSource::Publisher => Ok("publisher"),
        TranscriptSource::Scribe => Ok("scribeV1"),
        TranscriptSource::Whisper => Ok("whisper"),
        TranscriptSource::OnDevice => Ok("onDevice"),
        TranscriptSource::AssemblyAi => Ok("assemblyAI"),
        TranscriptSource::Other | TranscriptSource::Unsupported { .. } => {
            Err(StorageError::InvalidTranscriptArtifact)
        }
    }
}

pub(crate) fn uuid_string(bytes: [u8; 16]) -> String {
    let hex = hex_id(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

pub(crate) fn hex_id(bytes: [u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn seconds(milliseconds: u64) -> f64 {
    milliseconds as f64 / 1_000.0
}

fn timestamp(milliseconds: i64) -> Result<String, StorageError> {
    let nanoseconds = i128::from(milliseconds) * 1_000_000;
    let value = OffsetDateTime::from_unix_timestamp_nanos(nanoseconds)
        .map_err(|_| StorageError::InvalidTranscriptArtifact)?;
    value
        .format(&Rfc3339)
        .map_err(|_| StorageError::InvalidTranscriptArtifact)
}
