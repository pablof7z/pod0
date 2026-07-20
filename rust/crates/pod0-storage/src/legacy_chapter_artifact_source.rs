use std::fs;

use pod0_domain::{ChapterArtifactSource, ChapterLegacySource, ContentDigest, EpisodeId};

use crate::legacy_chapter_db::LegacyChapterArtifactRow;
use crate::legacy_chapter_episode::ChapterEpisodeContext;
use crate::legacy_chapter_format::{RawAdSpan, RawChapter};
use crate::legacy_chapter_transform::{
    ChapterTransformRequest, combined_payload_digest, transform_chapter_artifact,
};
use crate::legacy_format::{finite_milliseconds, uuid_bytes};
use crate::transcript_import_digest::{TranscriptImportHash, digest_bytes, hex_digest};
use crate::{
    ChapterEvidenceKind, ChapterEvidenceValidation, InspectedChapterEvidence, LegacyAdSpanIdentity,
    LegacyChapterSourceKind,
};

const MAX_WORKFLOW_ARTIFACT_BYTES: u64 = 16 * 1_024 * 1_024;

#[derive(Clone, Debug)]
pub(crate) enum LoadedWorkflowPayload {
    Chapters(Vec<RawChapter>),
    AdSpans(Vec<RawAdSpan>),
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedWorkflowArtifact {
    pub(crate) bytes: Vec<u8>,
    pub(crate) digest: ContentDigest,
    pub(crate) payload: Option<LoadedWorkflowPayload>,
    pub(crate) diagnostic_code: Option<String>,
}

pub(crate) fn load_workflow_artifact(row: &LegacyChapterArtifactRow) -> LoadedWorkflowArtifact {
    let empty = || LoadedWorkflowArtifact {
        bytes: Vec::new(),
        digest: digest_bytes(&[]),
        payload: None,
        diagnostic_code: Some("workflow_file_missing".to_owned()),
    };
    let Some(path) = row.location.as_deref() else {
        return empty();
    };
    if !path.is_absolute() {
        return LoadedWorkflowArtifact {
            diagnostic_code: Some("workflow_file_path_relative".to_owned()),
            ..empty()
        };
    }
    let Ok(metadata) = fs::metadata(path) else {
        return empty();
    };
    if metadata.len() > MAX_WORKFLOW_ARTIFACT_BYTES {
        return LoadedWorkflowArtifact {
            diagnostic_code: Some("workflow_file_too_large".to_owned()),
            ..empty()
        };
    }
    let Ok(bytes) = fs::read(path) else {
        return empty();
    };
    if bytes.len() as u64 != metadata.len() {
        return LoadedWorkflowArtifact {
            bytes,
            diagnostic_code: Some("workflow_file_changed_during_read".to_owned()),
            ..empty()
        };
    }
    let digest = digest_bytes(&bytes);
    if hex_digest(digest) != row.content_hash {
        return LoadedWorkflowArtifact {
            bytes,
            digest,
            payload: None,
            diagnostic_code: Some("workflow_content_hash_mismatch".to_owned()),
        };
    }
    let payload = match row.kind.as_str() {
        "chapters" => serde_json::from_slice(&bytes)
            .map(LoadedWorkflowPayload::Chapters)
            .ok(),
        "adSegments" => serde_json::from_slice(&bytes)
            .map(LoadedWorkflowPayload::AdSpans)
            .ok(),
        _ => None,
    };
    let diagnostic_code = payload
        .is_none()
        .then(|| "workflow_payload_invalid".to_owned());
    LoadedWorkflowArtifact {
        bytes,
        digest,
        payload,
        diagnostic_code,
    }
}

pub(crate) fn workflow_evidence(
    row: &LegacyChapterArtifactRow,
    loaded: &LoadedWorkflowArtifact,
    paired_ad: Option<&LoadedWorkflowArtifact>,
    context: Option<&ChapterEpisodeContext>,
    source_kind: LegacyChapterSourceKind,
    index: u32,
) -> InspectedChapterEvidence {
    let kind = if row.kind == "chapters" {
        ChapterEvidenceKind::WorkflowChapters
    } else {
        ChapterEvidenceKind::WorkflowAdSpans
    };
    let episode_id = uuid_bytes(&row.subject, "chapter artifact episode", index)
        .ok()
        .map(EpisodeId::from_bytes);
    let podcast_id = context.map(|context| context.podcast_id);
    let verified_at_ms =
        finite_milliseconds(row.verified_at_seconds, "chapter artifact", index).ok();
    let mut entry = base_entry(row, loaded, kind, episode_id, podcast_id, verified_at_ms);
    if row.schema_version != 1
        || !matches!(row.integrity.as_str(), "available" | "stale")
        || loaded.diagnostic_code.is_some()
        || episode_id.is_none()
        || podcast_id.is_none()
        || verified_at_ms.is_none()
        || (row.importer_selected && row.integrity != "available")
    {
        entry.validation = ChapterEvidenceValidation::Blocked;
        entry.diagnostic_code = loaded
            .diagnostic_code
            .clone()
            .or_else(|| Some("workflow_row_invalid".to_owned()));
        return entry;
    }
    match &loaded.payload {
        Some(LoadedWorkflowPayload::AdSpans(spans)) => {
            entry.legacy_ad_spans = ad_identities(spans, index);
            entry.validation = ChapterEvidenceValidation::Inert;
        }
        Some(LoadedWorkflowPayload::Chapters(chapters)) => {
            let Some(source) = classify_source(row.origin.as_deref(), chapters, context) else {
                entry.validation = ChapterEvidenceValidation::Blocked;
                entry.diagnostic_code = Some("workflow_origin_unsupported".to_owned());
                return entry;
            };
            let paired = paired_ad.and_then(|loaded| match &loaded.payload {
                Some(LoadedWorkflowPayload::AdSpans(spans)) if loaded.diagnostic_code.is_none() => {
                    Some((spans.as_slice(), loaded.digest))
                }
                _ => None,
            });
            let transformed = transform_chapter_artifact(
                ChapterTransformRequest {
                    episode_id: episode_id.expect("validated"),
                    podcast_id: podcast_id.expect("validated"),
                    source_revision: row.input_version.clone(),
                    source,
                    source_payload_digest: combined_payload_digest(
                        loaded.digest,
                        paired.map(|(_, digest)| digest),
                    ),
                    original_origin: row.origin.clone(),
                    legacy_source: match source_kind {
                        LegacyChapterSourceKind::ArtifactSqliteV0 => {
                            ChapterLegacySource::WorkflowArtifactV0
                        }
                        LegacyChapterSourceKind::ArtifactSqliteV1 => {
                            ChapterLegacySource::WorkflowArtifactV1
                        }
                    },
                    generated_at_ms: verified_at_ms.expect("validated"),
                    generated_at_was_unknown: false,
                    duration_ms: None,
                    chapters,
                    ad_spans: paired.map(|(spans, _)| spans),
                },
                index,
            );
            match transformed {
                Ok(transformed) => {
                    entry.validation = ChapterEvidenceValidation::Canonical;
                    entry.artifact = Some(transformed.artifact);
                    entry.legacy_chapters = transformed.legacy_chapters;
                    entry.legacy_ad_spans = transformed.legacy_ad_spans;
                }
                Err(_) => {
                    entry.validation = ChapterEvidenceValidation::Blocked;
                    entry.diagnostic_code = Some("workflow_contract_invalid".to_owned());
                }
            }
        }
        None => {
            entry.validation = ChapterEvidenceValidation::Blocked;
            entry.diagnostic_code = Some("workflow_payload_invalid".to_owned());
        }
    }
    entry
}

fn base_entry(
    row: &LegacyChapterArtifactRow,
    loaded: &LoadedWorkflowArtifact,
    kind: ChapterEvidenceKind,
    episode_id: Option<EpisodeId>,
    podcast_id: Option<pod0_domain::PodcastId>,
    verified_at_ms: Option<i64>,
) -> InspectedChapterEvidence {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-chapter-evidence.v1");
    hash.text(kind.code());
    hash.bytes(&row.row_digest.into_bytes());
    hash.bytes(&loaded.digest.into_bytes());
    InspectedChapterEvidence {
        evidence_id: hash.finish(),
        kind,
        source_subject: row.subject.clone(),
        episode_id,
        podcast_id,
        source_row_id: Some(row.row_id),
        legacy_selected: row.legacy_selected,
        importer_selected: row.importer_selected,
        source_input_version: Some(row.input_version.clone()),
        source_output_version: Some(row.output_version.clone()),
        source_origin: row.origin.clone(),
        source_schema_version: u32::try_from(row.schema_version).ok(),
        source_integrity: Some(row.integrity.clone()),
        source_verified_at_ms: verified_at_ms,
        source_path: row.location.clone(),
        source_row_digest: row.row_digest,
        raw_digest: loaded.digest,
        raw_byte_count: loaded.bytes.len() as u64,
        raw_bytes: loaded.bytes.clone(),
        validation: ChapterEvidenceValidation::Inert,
        diagnostic_code: None,
        artifact: None,
        legacy_chapters: Vec::new(),
        legacy_ad_spans: Vec::new(),
    }
}

fn classify_source(
    origin: Option<&str>,
    chapters: &[RawChapter],
    context: Option<&ChapterEpisodeContext>,
) -> Option<ChapterArtifactSource> {
    if context.is_some_and(|context| context.is_agent_generated)
        && chapters.iter().all(|chapter| chapter.is_ai_generated)
    {
        return Some(ChapterArtifactSource::AgentComposed);
    }
    match origin {
        Some(value) if value.starts_with("publisherEnriched") => {
            Some(ChapterArtifactSource::PublisherEnriched)
        }
        Some(value) if value.starts_with("publisher:") => Some(ChapterArtifactSource::Publisher),
        Some("generated") => Some(ChapterArtifactSource::Generated),
        _ => None,
    }
}

fn ad_identities(spans: &[RawAdSpan], index: u32) -> Vec<LegacyAdSpanIdentity> {
    spans
        .iter()
        .enumerate()
        .map(|(ordinal, span)| LegacyAdSpanIdentity {
            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
            legacy_id: span
                .id
                .as_deref()
                .and_then(|value| uuid_bytes(value, "ad-span identity", index).ok()),
            ad_span_id: None,
        })
        .collect()
}
