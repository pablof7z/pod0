use pod0_domain::{StateRevision, TranscriptArtifact, TranscriptArtifactSpeakerInput};

use crate::{
    MAX_OPERATION_ITEMS, MAX_PROJECTION_ITEMS, TranscriptProjection, TranscriptProjectionScope,
    TranscriptSegmentProjection, TranscriptSpeakerProjection, TranscriptSummaryProjection,
    TranscriptWordProjection,
};

impl TranscriptProjection {
    pub fn enforce_bounds(&mut self, offset: usize, requested_items: usize) {
        let limit = requested_items.clamp(1, usize::from(MAX_PROJECTION_ITEMS));
        let count = match self.scope {
            TranscriptProjectionScope::Summary => 0,
            TranscriptProjectionScope::Speakers => page(&mut self.speakers, offset, limit),
            TranscriptProjectionScope::Segments | TranscriptProjectionScope::Segment { .. } => {
                page(&mut self.segments, offset, limit)
            }
            TranscriptProjectionScope::Words { .. } => page(&mut self.words, offset, limit),
            TranscriptProjectionScope::Unsupported { .. } => {
                self.summary = None;
                self.speakers.clear();
                self.segments.clear();
                self.words.clear();
                0
            }
        };
        self.operations.truncate(MAX_OPERATION_ITEMS);
        self.has_more |= count > offset.saturating_add(limit);
    }
}

pub fn project_transcript_artifact(
    artifact: &TranscriptArtifact,
    selection_revision: StateRevision,
    scope: TranscriptProjectionScope,
) -> TranscriptProjection {
    let word_count = artifact
        .segments
        .iter()
        .map(|segment| segment.words.len() as u64)
        .sum();
    let summary = TranscriptSummaryProjection {
        artifact_id: artifact.artifact_id,
        transcript_version_id: artifact.transcript_version_id,
        episode_id: artifact.episode_id,
        podcast_id: artifact.podcast_id,
        source: artifact.provenance.source,
        provider: artifact.provenance.provider.clone(),
        source_payload_digest: artifact.provenance.source_payload_digest,
        language: artifact.language.clone(),
        generated_at: artifact.generated_at,
        transcript_content_digest: artifact.content_digest,
        artifact_integrity_digest: artifact.integrity_digest,
        selection_revision,
        speaker_count: artifact.speakers.len() as u32,
        segment_count: artifact.segments.len() as u32,
        word_count,
    };
    let speakers = if matches!(scope, TranscriptProjectionScope::Speakers) {
        artifact.speakers.iter().map(speaker_projection).collect()
    } else {
        Vec::new()
    };
    let segments = artifact
        .segments
        .iter()
        .filter(|segment| match scope {
            TranscriptProjectionScope::Segments => true,
            TranscriptProjectionScope::Segment { segment_id } => segment.segment_id == segment_id,
            _ => false,
        })
        .map(|segment| TranscriptSegmentProjection {
            segment_id: segment.segment_id,
            ordinal: segment.ordinal,
            text: segment.text.clone(),
            start_milliseconds: segment.start_milliseconds,
            end_milliseconds: segment.end_milliseconds,
            speaker_id: segment.speaker_id,
            word_count: segment.words.len() as u32,
        })
        .collect();
    let words = artifact
        .segments
        .iter()
        .filter(|segment| {
            matches!(scope, TranscriptProjectionScope::Words { segment_id } if segment.segment_id == segment_id)
        })
        .flat_map(|segment| {
            segment.words.iter().enumerate().map(move |(ordinal, word)| {
                TranscriptWordProjection {
                    segment_id: segment.segment_id,
                    ordinal: ordinal as u32,
                    text: word.text.clone(),
                    start_milliseconds: word.start_milliseconds,
                    end_milliseconds: word.end_milliseconds,
                }
            })
        })
        .collect();
    TranscriptProjection {
        scope,
        summary: Some(summary),
        speakers,
        segments,
        words,
        operations: Vec::new(),
        has_more: false,
    }
}

fn speaker_projection(value: &TranscriptArtifactSpeakerInput) -> TranscriptSpeakerProjection {
    TranscriptSpeakerProjection {
        speaker_id: value.speaker_id,
        label: value.label.clone(),
        display_name: value.display_name.clone(),
    }
}

fn page<T>(values: &mut Vec<T>, offset: usize, limit: usize) -> usize {
    let count = values.len();
    *values = std::mem::take(values)
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();
    count
}
