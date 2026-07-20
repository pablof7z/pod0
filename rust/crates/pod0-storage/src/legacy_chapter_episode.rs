use pod0_domain::{ChapterArtifactSource, ChapterLegacySource, EpisodeId, PodcastId};
use serde_json::Value;

use crate::legacy_chapter_db::LegacyChapterEpisodeRow;
use crate::legacy_chapter_format::RawChapterEpisode;
use crate::legacy_chapter_transform::{ChapterTransformRequest, transform_chapter_artifact};
use crate::legacy_format::{finite_milliseconds, timestamp_milliseconds, uuid_bytes};
use crate::transcript_import_digest::{TranscriptImportHash, hex_digest};
use crate::{
    ChapterEvidenceKind, ChapterEvidenceValidation, InspectedChapterEvidence,
    LegacyChapterSourceKind, StorageError,
};

#[derive(Clone, Debug)]
pub(crate) struct ChapterEpisodeContext {
    pub(crate) episode_id: EpisodeId,
    pub(crate) podcast_id: PodcastId,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) is_agent_generated: bool,
}

pub(crate) fn inspect_episode_adjunct(
    row: &LegacyChapterEpisodeRow,
    source_kind: LegacyChapterSourceKind,
    source_generation: u64,
    index: u32,
) -> (
    Option<ChapterEpisodeContext>,
    Option<InspectedChapterEvidence>,
) {
    let value: Value = match serde_json::from_slice(&row.payload) {
        Ok(value) => value,
        Err(_) => return (None, Some(blocked(row, "episode_payload_invalid"))),
    };
    let has_adjunct = value.get("chapters").is_some() || value.get("adSegments").is_some();
    let raw: RawChapterEpisode = match serde_json::from_value(value) {
        Ok(raw) => raw,
        Err(_) if has_adjunct => return (None, Some(blocked(row, "episode_adjunct_invalid"))),
        Err(_) => return (None, None),
    };
    let context = match context(&raw, row, index) {
        Ok(context) => context,
        Err(_) if has_adjunct => return (None, Some(blocked(row, "episode_identity_invalid"))),
        Err(_) => return (None, None),
    };
    if !has_adjunct {
        return (Some(context), None);
    }
    let mut entry = base_entry(row, Some(context.episode_id), Some(context.podcast_id));
    let Some(chapters) = raw.chapters.as_deref() else {
        entry.validation = ChapterEvidenceValidation::Blocked;
        entry.diagnostic_code = Some("episode_chapters_missing".to_owned());
        return (Some(context), Some(entry));
    };
    let source = episode_source(&raw);
    let generated_at_was_unknown = !context.is_agent_generated;
    let generated_at_ms = if generated_at_was_unknown {
        0
    } else {
        match timestamp_milliseconds(raw.published_at.as_ref(), "chapter episode", index) {
            Ok(value) => value,
            Err(_) => {
                entry.validation = ChapterEvidenceValidation::Blocked;
                entry.diagnostic_code = Some("episode_generation_time_invalid".to_owned());
                return (Some(context), Some(entry));
            }
        }
    };
    let original_origin = raw.generation_source.as_ref().map(|generation| {
        format!(
            "{}:{}",
            generation.kind,
            generation.conversation_id.as_deref().unwrap_or("unknown")
        )
    });
    let transformed = transform_chapter_artifact(
        ChapterTransformRequest {
            episode_id: context.episode_id,
            podcast_id: context.podcast_id,
            source_revision: format!(
                "legacy-episode:{source_generation}:{}",
                hex_digest(row.payload_digest)
            ),
            source,
            source_payload_digest: row.payload_digest,
            original_origin,
            legacy_source: match source_kind {
                LegacyChapterSourceKind::ArtifactSqliteV0
                | LegacyChapterSourceKind::ArtifactSqliteV1 => ChapterLegacySource::EpisodeAdjunct,
            },
            generated_at_ms,
            generated_at_was_unknown,
            duration_ms: context.duration_ms,
            chapters,
            ad_spans: raw.ad_spans.as_deref(),
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
            entry.diagnostic_code = Some("episode_adjunct_contract_invalid".to_owned());
        }
    }
    (Some(context), Some(entry))
}

fn context(
    raw: &RawChapterEpisode,
    row: &LegacyChapterEpisodeRow,
    index: u32,
) -> Result<ChapterEpisodeContext, StorageError> {
    let parent = raw
        .podcast_id
        .as_deref()
        .or(raw.legacy_subscription_id.as_deref())
        .ok_or_else(|| invalid(index, "episode parent is missing"))?;
    let episode_id = EpisodeId::from_bytes(uuid_bytes(&raw.id, "chapter episode", index)?);
    let podcast_id = PodcastId::from_bytes(uuid_bytes(parent, "chapter podcast", index)?);
    if uuid_bytes(&row.subject, "chapter episode row", index)? != episode_id.into_bytes()
        || uuid_bytes(&row.parent, "chapter podcast row", index)? != podcast_id.into_bytes()
    {
        return Err(invalid(index, "episode row and payload identities differ"));
    }
    let duration_ms = raw
        .duration
        .map(|value| finite_milliseconds(value, "chapter episode", index))
        .transpose()?
        .map(|value| u64::try_from(value).map_err(|_| invalid(index, "duration is invalid")))
        .transpose()?;
    let is_agent_generated = raw.generation_source.as_ref().is_some_and(|generation| {
        generation.kind == "inAppChat"
            && generation
                .conversation_id
                .as_deref()
                .and_then(|value| uuid_bytes(value, "chapter conversation", index).ok())
                .is_some()
    });
    if raw.generation_source.is_some() && !is_agent_generated {
        return Err(invalid(index, "generation source is invalid"));
    }
    Ok(ChapterEpisodeContext {
        episode_id,
        podcast_id,
        duration_ms,
        is_agent_generated,
    })
}

fn episode_source(raw: &RawChapterEpisode) -> ChapterArtifactSource {
    if raw.generation_source.is_some() {
        ChapterArtifactSource::AgentComposed
    } else if raw
        .chapters
        .as_deref()
        .is_some_and(|chapters| chapters.iter().all(|chapter| chapter.is_ai_generated))
    {
        ChapterArtifactSource::Generated
    } else if raw.chapters.as_deref().is_some_and(|chapters| {
        chapters
            .iter()
            .any(|chapter| chapter.is_ai_generated || chapter.summary.is_some())
    }) {
        ChapterArtifactSource::PublisherEnriched
    } else {
        ChapterArtifactSource::Publisher
    }
}

fn base_entry(
    row: &LegacyChapterEpisodeRow,
    episode_id: Option<EpisodeId>,
    podcast_id: Option<PodcastId>,
) -> InspectedChapterEvidence {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-chapter-evidence.v1");
    hash.text(ChapterEvidenceKind::EpisodeAdjunct.code());
    hash.text(&row.subject);
    hash.bytes(&row.payload_digest.into_bytes());
    InspectedChapterEvidence {
        evidence_id: hash.finish(),
        kind: ChapterEvidenceKind::EpisodeAdjunct,
        source_subject: row.subject.clone(),
        episode_id,
        podcast_id,
        source_row_id: None,
        legacy_selected: None,
        importer_selected: false,
        source_input_version: None,
        source_output_version: None,
        source_origin: None,
        source_schema_version: None,
        source_integrity: None,
        source_verified_at_ms: None,
        source_path: None,
        source_row_digest: row.payload_digest,
        raw_digest: row.payload_digest,
        raw_byte_count: row.payload.len() as u64,
        raw_bytes: row.payload.clone(),
        validation: ChapterEvidenceValidation::Inert,
        diagnostic_code: None,
        artifact: None,
        legacy_chapters: Vec::new(),
        legacy_ad_spans: Vec::new(),
    }
}

fn blocked(row: &LegacyChapterEpisodeRow, code: &'static str) -> InspectedChapterEvidence {
    let mut entry = base_entry(row, None, None);
    entry.validation = ChapterEvidenceValidation::Blocked;
    entry.diagnostic_code = Some(code.to_owned());
    entry
}

fn invalid(index: u32, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "chapter episode",
        index,
        detail,
    }
}
