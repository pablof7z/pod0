use crate::chapter_artifact_validation::{CanonicalAdSpan, CanonicalChapter};
use crate::knowledge_identity::{StableHash, first_16};
use crate::{
    AdSpanEvaluation, AdSpanId, ChapterAdKind, ChapterArtifact, ChapterArtifactId,
    ChapterArtifactProvenance, ChapterArtifactSource, ChapterId, ContentDigest, EpisodeId,
    PodcastId, StateRevision,
};

pub(crate) fn chapter_artifact_content_digest(
    duration_milliseconds: Option<u64>,
    chapters: &[CanonicalChapter],
    ad_evaluation: AdSpanEvaluation,
    ad_spans: &[CanonicalAdSpan],
) -> ContentDigest {
    let mut hash = StableHash::new(b"pod0.chapter-content.v1");
    optional_u64(&mut hash, duration_milliseconds);
    hash.u64(chapters.len() as u64);
    for chapter in chapters {
        hash_chapter(&mut hash, chapter);
    }
    hash_ad_evaluation(&mut hash, ad_evaluation);
    hash.u64(ad_spans.len() as u64);
    for span in ad_spans {
        hash_ad_span(&mut hash, span);
    }
    ContentDigest::from_bytes(hash.finish())
}

pub(crate) fn chapter_artifact_id(
    episode_id: EpisodeId,
    podcast_id: PodcastId,
    source_revision: &str,
    provenance: &ChapterArtifactProvenance,
    content_digest: ContentDigest,
) -> ChapterArtifactId {
    let mut hash = StableHash::new(b"pod0.chapter-artifact-id.v1");
    hash.bytes(&episode_id.into_bytes());
    hash.bytes(&podcast_id.into_bytes());
    hash.text(source_revision);
    hash_provenance(&mut hash, provenance);
    hash.bytes(&content_digest.into_bytes());
    ChapterArtifactId::from_bytes(first_16(hash.finish()))
}

pub(crate) fn chapter_id(artifact_id: ChapterArtifactId, chapter: &CanonicalChapter) -> ChapterId {
    let mut hash = StableHash::new(b"pod0.chapter-id.v1");
    hash.bytes(&artifact_id.into_bytes());
    hash_chapter(&mut hash, chapter);
    ChapterId::from_bytes(first_16(hash.finish()))
}

pub(crate) fn ad_span_id(artifact_id: ChapterArtifactId, span: &CanonicalAdSpan) -> AdSpanId {
    let mut hash = StableHash::new(b"pod0.ad-span-id.v1");
    hash.bytes(&artifact_id.into_bytes());
    hash_ad_span(&mut hash, span);
    AdSpanId::from_bytes(first_16(hash.finish()))
}

pub(crate) fn chapter_artifact_digest(artifact: &ChapterArtifact) -> ContentDigest {
    let mut hash = StableHash::new(b"pod0.chapter-artifact.v1");
    hash.u32(artifact.schema_version);
    hash.bytes(&artifact.artifact_id.into_bytes());
    hash.bytes(&artifact.content_digest.into_bytes());
    hash.bytes(&artifact.episode_id.into_bytes());
    hash.bytes(&artifact.podcast_id.into_bytes());
    hash.text(&artifact.source_revision);
    hash_provenance(&mut hash, &artifact.provenance);
    hash.i64(artifact.generated_at.value);
    optional_u64(&mut hash, artifact.duration_milliseconds);
    hash.u64(artifact.chapters.len() as u64);
    for chapter in &artifact.chapters {
        hash.bytes(&chapter.chapter_id.into_bytes());
        hash.u32(chapter.ordinal);
        hash.u64(chapter.start_milliseconds);
        optional_u64(&mut hash, chapter.end_milliseconds);
        hash.text(&chapter.title);
        hash.optional_text(chapter.summary.as_deref());
        hash.optional_text(chapter.image_url.as_deref());
        hash.optional_text(chapter.link_url.as_deref());
        hash.u8(u8::from(chapter.include_in_table_of_contents));
        hash.optional_id(chapter.source_episode_id.map(EpisodeId::into_bytes));
    }
    hash_ad_evaluation(&mut hash, artifact.ad_span_evaluation);
    hash.u64(artifact.ad_spans.len() as u64);
    for span in &artifact.ad_spans {
        hash.bytes(&span.ad_span_id.into_bytes());
        hash.u32(span.ordinal);
        hash.u64(span.start_milliseconds);
        hash.u64(span.end_milliseconds);
        hash_ad_kind(&mut hash, span.kind);
    }
    ContentDigest::from_bytes(hash.finish())
}

pub(crate) fn chapter_command_fingerprint(
    expected_revision: StateRevision,
    artifact: &ChapterArtifact,
) -> ContentDigest {
    let mut hash = StableHash::new(b"pod0.commit-chapter-artifact.v1");
    hash.u64(expected_revision.value);
    hash.bytes(&artifact.artifact_id.into_bytes());
    hash.bytes(&artifact.integrity_digest.into_bytes());
    ContentDigest::from_bytes(hash.finish())
}

fn hash_chapter(hash: &mut StableHash, chapter: &CanonicalChapter) {
    hash.u32(chapter.ordinal);
    hash.u64(chapter.start_milliseconds);
    optional_u64(hash, chapter.end_milliseconds);
    hash.text(&chapter.title);
    hash.optional_text(chapter.summary.as_deref());
    hash.optional_text(chapter.image_url.as_deref());
    hash.optional_text(chapter.link_url.as_deref());
    hash.u8(u8::from(chapter.include_in_table_of_contents));
    hash.optional_id(chapter.source_episode_id.map(EpisodeId::into_bytes));
}

fn hash_ad_span(hash: &mut StableHash, span: &CanonicalAdSpan) {
    hash.u32(span.ordinal);
    hash.u64(span.start_milliseconds);
    hash.u64(span.end_milliseconds);
    hash_ad_kind(hash, span.kind);
}

fn hash_provenance(hash: &mut StableHash, provenance: &ChapterArtifactProvenance) {
    hash_source(hash, provenance.source);
    hash.optional_text(provenance.provider.as_deref());
    hash.optional_text(provenance.model.as_deref());
    hash.u32(provenance.policy_version);
    hash.bytes(&provenance.source_payload_digest.into_bytes());
    hash.optional_id(
        provenance
            .transcript_version_id
            .map(crate::TranscriptVersionId::into_bytes),
    );
    optional_digest(hash, provenance.transcript_content_digest);
    if let Some(legacy) = &provenance.legacy_import {
        hash.u8(1);
        hash_legacy_source(hash, legacy.source);
        hash.optional_text(legacy.original_origin.as_deref());
        hash.u8(u8::from(legacy.generated_at_was_unknown));
    }
}

fn hash_legacy_source(hash: &mut StableHash, source: crate::ChapterLegacySource) {
    match source {
        crate::ChapterLegacySource::EpisodeAdjunct => hash.u32(1),
        crate::ChapterLegacySource::WorkflowArtifactV0 => hash.u32(2),
        crate::ChapterLegacySource::WorkflowArtifactV1 => hash.u32(3),
        crate::ChapterLegacySource::Unsupported { wire_code } => {
            hash.u32(u32::MAX);
            hash.u32(wire_code);
        }
    }
}

fn hash_source(hash: &mut StableHash, source: ChapterArtifactSource) {
    match source {
        ChapterArtifactSource::Publisher => hash.u32(1),
        ChapterArtifactSource::Generated => hash.u32(2),
        ChapterArtifactSource::PublisherEnriched => hash.u32(3),
        ChapterArtifactSource::AgentComposed => hash.u32(4),
        ChapterArtifactSource::Unsupported { wire_code } => {
            hash.u32(u32::MAX);
            hash.u32(wire_code);
        }
    }
}

fn hash_ad_kind(hash: &mut StableHash, kind: ChapterAdKind) {
    match kind {
        ChapterAdKind::Preroll => hash.u32(1),
        ChapterAdKind::Midroll => hash.u32(2),
        ChapterAdKind::Postroll => hash.u32(3),
        ChapterAdKind::Unsupported { wire_code } => {
            hash.u32(u32::MAX);
            hash.u32(wire_code);
        }
    }
}

fn hash_ad_evaluation(hash: &mut StableHash, value: AdSpanEvaluation) {
    match value {
        AdSpanEvaluation::NotEvaluated => hash.u32(1),
        AdSpanEvaluation::Evaluated => hash.u32(2),
        AdSpanEvaluation::Unsupported { wire_code } => {
            hash.u32(u32::MAX);
            hash.u32(wire_code);
        }
    }
}

fn optional_u64(hash: &mut StableHash, value: Option<u64>) {
    match value {
        Some(value) => {
            hash.u8(1);
            hash.u64(value);
        }
        None => hash.u8(0),
    }
}

fn optional_digest(hash: &mut StableHash, value: Option<ContentDigest>) {
    match value {
        Some(value) => {
            hash.u8(1);
            hash.bytes(&value.into_bytes());
        }
        None => hash.u8(0),
    }
}
