use pod0_domain::{
    AdSpanEvaluation, AdSpanInput, ChapterAdKind, ChapterArtifact, ChapterArtifactInput,
    ChapterArtifactProvenance, ChapterArtifactSource, ChapterInput, ChapterLegacyProvenance,
    ChapterLegacySource, ContentDigest, EpisodeId, PodcastId, UnixTimestampMilliseconds,
};

use crate::legacy_chapter_format::{RawAdSpan, RawChapter};
use crate::legacy_format::{finite_milliseconds, uuid_bytes};
use crate::transcript_import_digest::TranscriptImportHash;
use crate::{LegacyAdSpanIdentity, LegacyChapterIdentity, StorageError};

pub(crate) struct ChapterTransformRequest<'a> {
    pub(crate) episode_id: EpisodeId,
    pub(crate) podcast_id: PodcastId,
    pub(crate) source_revision: String,
    pub(crate) source: ChapterArtifactSource,
    pub(crate) source_payload_digest: ContentDigest,
    pub(crate) original_origin: Option<String>,
    pub(crate) legacy_source: ChapterLegacySource,
    pub(crate) generated_at_ms: i64,
    pub(crate) generated_at_was_unknown: bool,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) chapters: &'a [RawChapter],
    pub(crate) ad_spans: Option<&'a [RawAdSpan]>,
}

pub(crate) struct TransformedChapterArtifact {
    pub(crate) artifact: ChapterArtifact,
    pub(crate) legacy_chapters: Vec<LegacyChapterIdentity>,
    pub(crate) legacy_ad_spans: Vec<LegacyAdSpanIdentity>,
}

pub(crate) fn transform_chapter_artifact(
    request: ChapterTransformRequest<'_>,
    index: u32,
) -> Result<TransformedChapterArtifact, StorageError> {
    let chapter_inputs = request
        .chapters
        .iter()
        .map(|chapter| chapter_input(chapter, index))
        .collect::<Result<Vec<_>, _>>()?;
    let ad_inputs = request
        .ad_spans
        .unwrap_or_default()
        .iter()
        .map(|span| ad_input(span, index))
        .collect::<Result<Vec<_>, _>>()?;
    let artifact = ChapterArtifact::seal(ChapterArtifactInput {
        episode_id: request.episode_id,
        podcast_id: request.podcast_id,
        source_revision: request.source_revision,
        provenance: ChapterArtifactProvenance {
            source: request.source,
            provider: None,
            model: None,
            policy_version: 0,
            source_payload_digest: request.source_payload_digest,
            transcript_version_id: None,
            transcript_content_digest: None,
            legacy_import: Some(ChapterLegacyProvenance {
                source: request.legacy_source,
                original_origin: request.original_origin,
                generated_at_was_unknown: request.generated_at_was_unknown,
            }),
        },
        generated_at: UnixTimestampMilliseconds::new(request.generated_at_ms),
        duration_milliseconds: request.duration_ms,
        chapters: chapter_inputs,
        ad_span_evaluation: if request.ad_spans.is_some() {
            AdSpanEvaluation::Evaluated
        } else {
            AdSpanEvaluation::NotEvaluated
        },
        ad_spans: ad_inputs,
    })
    .map_err(|_| invalid(index, "legacy chapters violate the canonical contract"))?;
    let legacy_chapters = request
        .chapters
        .iter()
        .zip(&artifact.chapters)
        .enumerate()
        .map(|(ordinal, (legacy, canonical))| {
            Ok(LegacyChapterIdentity {
                ordinal: u32::try_from(ordinal)
                    .map_err(|_| invalid(index, "legacy chapter count is invalid"))?,
                legacy_id: optional_uuid(legacy.id.as_deref(), "chapter identity", index)?,
                is_ai_generated: legacy.is_ai_generated,
                chapter_id: Some(canonical.chapter_id),
            })
        })
        .collect::<Result<_, StorageError>>()?;
    let legacy_ad_spans = request
        .ad_spans
        .unwrap_or_default()
        .iter()
        .zip(&artifact.ad_spans)
        .enumerate()
        .map(|(ordinal, (legacy, canonical))| {
            Ok(LegacyAdSpanIdentity {
                ordinal: u32::try_from(ordinal)
                    .map_err(|_| invalid(index, "legacy ad-span count is invalid"))?,
                legacy_id: optional_uuid(legacy.id.as_deref(), "ad-span identity", index)?,
                ad_span_id: Some(canonical.ad_span_id),
            })
        })
        .collect::<Result<_, StorageError>>()?;
    Ok(TransformedChapterArtifact {
        artifact,
        legacy_chapters,
        legacy_ad_spans,
    })
}

pub(crate) fn combined_payload_digest(
    chapter_digest: ContentDigest,
    ad_digest: Option<ContentDigest>,
) -> ContentDigest {
    let mut hash = TranscriptImportHash::new(b"pod0.legacy-chapter-payload.v1");
    hash.bytes(&chapter_digest.into_bytes());
    match ad_digest {
        Some(digest) => {
            hash.u32(1);
            hash.bytes(&digest.into_bytes());
        }
        None => hash.u32(0),
    }
    hash.finish()
}

fn chapter_input(raw: &RawChapter, index: u32) -> Result<ChapterInput, StorageError> {
    Ok(ChapterInput {
        start_milliseconds: milliseconds(raw.start_time, index)?,
        end_milliseconds: raw
            .end_time
            .map(|value| milliseconds(value, index))
            .transpose()?,
        title: raw.title.clone(),
        summary: raw.summary.clone(),
        image_url: raw.image_url.clone(),
        link_url: raw.link_url.clone(),
        include_in_table_of_contents: raw.include_in_table_of_contents,
        source_episode_id: raw
            .source_episode_id
            .as_deref()
            .map(|value| {
                uuid_bytes(value, "chapter source episode", index).map(EpisodeId::from_bytes)
            })
            .transpose()?,
    })
}

fn ad_input(raw: &RawAdSpan, index: u32) -> Result<AdSpanInput, StorageError> {
    let kind = match raw.kind.as_str() {
        "preroll" => ChapterAdKind::Preroll,
        "midroll" => ChapterAdKind::Midroll,
        "postroll" => ChapterAdKind::Postroll,
        _ => return Err(invalid(index, "legacy ad kind is unsupported")),
    };
    Ok(AdSpanInput {
        start_milliseconds: milliseconds(raw.start, index)?,
        end_milliseconds: milliseconds(raw.end, index)?,
        kind,
    })
}

fn milliseconds(value: f64, index: u32) -> Result<u64, StorageError> {
    u64::try_from(finite_milliseconds(value, "chapter", index)?)
        .map_err(|_| invalid(index, "legacy chapter timestamp is outside supported range"))
}

fn optional_uuid(
    value: Option<&str>,
    entity: &'static str,
    index: u32,
) -> Result<Option<[u8; 16]>, StorageError> {
    value
        .map(|value| uuid_bytes(value, entity, index))
        .transpose()
}

fn invalid(index: u32, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "chapter",
        index,
        detail,
    }
}
