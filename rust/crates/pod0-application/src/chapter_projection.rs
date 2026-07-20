use pod0_domain::{ChapterArtifact, StateRevision};

use crate::{
    AdSpanProjection, ChapterArtifactProjection, ChapterItemProjection, ChapterProjectionScope,
    ChapterSummaryProjection, MAX_PROJECTION_ITEMS,
};

#[must_use]
pub fn project_chapter_artifact(
    artifact: &ChapterArtifact,
    selection_revision: StateRevision,
    scope: ChapterProjectionScope,
    offset: usize,
    requested_items: usize,
) -> ChapterArtifactProjection {
    let summary = ChapterSummaryProjection {
        artifact_id: artifact.artifact_id,
        episode_id: artifact.episode_id,
        podcast_id: artifact.podcast_id,
        source_revision: artifact.source_revision.clone(),
        provenance: artifact.provenance.clone(),
        generated_at: artifact.generated_at,
        duration_milliseconds: artifact.duration_milliseconds,
        content_digest: artifact.content_digest,
        integrity_digest: artifact.integrity_digest,
        selection_revision,
        chapter_count: artifact.chapters.len() as u32,
        ad_span_evaluation: artifact.ad_span_evaluation,
        ad_span_count: artifact.ad_spans.len() as u32,
    };
    let limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
    let mut projection = ChapterArtifactProjection {
        scope,
        summary: Some(summary),
        chapters: Vec::new(),
        ad_spans: Vec::new(),
        operations: Vec::new(),
        failure: None,
        has_more: false,
    };
    match scope {
        ChapterProjectionScope::Summary => {}
        ChapterProjectionScope::Chapters => {
            let total = artifact.chapters.len();
            projection.chapters = artifact
                .chapters
                .iter()
                .enumerate()
                .skip(offset)
                .take(limit)
                .map(|(index, chapter)| chapter_projection(artifact, index, chapter))
                .collect();
            projection.has_more = total > offset.saturating_add(limit);
        }
        ChapterProjectionScope::Chapter { chapter_id } => {
            projection.chapters = artifact
                .chapters
                .iter()
                .enumerate()
                .filter(|(_, chapter)| chapter.chapter_id == chapter_id)
                .skip(offset)
                .take(limit)
                .map(|(index, chapter)| chapter_projection(artifact, index, chapter))
                .collect();
        }
        ChapterProjectionScope::AdSpans => {
            let total = artifact.ad_spans.len();
            projection.ad_spans = artifact
                .ad_spans
                .iter()
                .skip(offset)
                .take(limit)
                .map(|span| AdSpanProjection {
                    ad_span_id: span.ad_span_id,
                    ordinal: span.ordinal,
                    start_milliseconds: span.start_milliseconds,
                    end_milliseconds: span.end_milliseconds,
                    kind: span.kind,
                })
                .collect();
            projection.has_more = total > offset.saturating_add(limit);
        }
        ChapterProjectionScope::Unsupported { .. } => {
            projection.summary = None;
        }
    }
    projection
}

fn chapter_projection(
    artifact: &ChapterArtifact,
    index: usize,
    chapter: &pod0_domain::ChapterRecord,
) -> ChapterItemProjection {
    let inferred_end = artifact
        .chapters
        .get(index + 1)
        .map(|next| next.start_milliseconds)
        .or(artifact.duration_milliseconds);
    ChapterItemProjection {
        chapter_id: chapter.chapter_id,
        ordinal: chapter.ordinal,
        start_milliseconds: chapter.start_milliseconds,
        explicit_end_milliseconds: chapter.end_milliseconds,
        effective_end_milliseconds: chapter.end_milliseconds.or(inferred_end),
        title: chapter.title.clone(),
        summary: chapter.summary.clone(),
        image_url: chapter.image_url.clone(),
        link_url: chapter.link_url.clone(),
        include_in_table_of_contents: chapter.include_in_table_of_contents,
        source_episode_id: chapter.source_episode_id,
    }
}
