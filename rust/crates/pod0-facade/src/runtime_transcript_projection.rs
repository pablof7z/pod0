use pod0_application::{
    CoreFailureCode, TranscriptProjection, TranscriptProjectionScope, TranscriptSegmentProjection,
    TranscriptSpeakerProjection, TranscriptSummaryProjection, TranscriptWordProjection,
};
use pod0_domain::EpisodeId;

use crate::runtime_state::{FacadeState, failure};
use crate::runtime_storage_commands::storage_failure;

impl FacadeState {
    pub(super) fn transcript_projection(
        &self,
        episode_id: EpisodeId,
        scope: TranscriptProjectionScope,
        offset: u32,
        max_items: u16,
    ) -> TranscriptProjection {
        let mut projection = empty_projection(scope, &self.operations);
        let Some(store) = &self.transcript_store else {
            projection.failure = Some(failure(CoreFailureCode::StorageUnavailable));
            return projection;
        };
        let summary = match store.selected_summary(episode_id) {
            Ok(Some(summary)) => summary,
            Ok(None) => return projection,
            Err(error) => {
                projection.failure = Some(failure(storage_failure(error)));
                return projection;
            }
        };
        projection.summary = Some(TranscriptSummaryProjection {
            artifact_id: summary.artifact_id,
            transcript_version_id: summary.transcript_version_id,
            episode_id: summary.episode_id,
            podcast_id: summary.podcast_id,
            source_revision: summary.source_revision,
            source: summary.source,
            provider: summary.provider,
            source_payload_digest: summary.source_payload_digest,
            language: summary.language,
            generated_at: summary.generated_at,
            transcript_content_digest: summary.transcript_content_digest,
            artifact_integrity_digest: summary.artifact_integrity_digest,
            selection_revision: summary.selection_revision,
            speaker_count: summary.speaker_count,
            segment_count: summary.segment_count,
            word_count: summary.word_count,
        });
        let result = match scope {
            TranscriptProjectionScope::Summary => Ok(false),
            TranscriptProjectionScope::Speakers => store
                .selected_speakers(episode_id, offset, max_items)
                .map(|page| {
                    projection.speakers = page
                        .items
                        .into_iter()
                        .map(|value| TranscriptSpeakerProjection {
                            speaker_id: value.speaker_id,
                            label: value.label,
                            display_name: value.display_name,
                        })
                        .collect();
                    page.has_more
                }),
            TranscriptProjectionScope::Segments => store
                .selected_segments(episode_id, offset, max_items)
                .map(|page| {
                    projection.segments = page.items.into_iter().map(segment).collect();
                    page.has_more
                }),
            TranscriptProjectionScope::Segment { segment_id } => {
                store.selected_segment(episode_id, segment_id).map(|value| {
                    projection.segments = value.into_iter().map(segment).collect();
                    false
                })
            }
            TranscriptProjectionScope::Words { segment_id } => store
                .selected_words(episode_id, segment_id, offset, max_items)
                .map(|page| {
                    projection.words = page
                        .items
                        .into_iter()
                        .map(|value| TranscriptWordProjection {
                            segment_id: value.segment_id,
                            ordinal: value.ordinal,
                            text: value.text,
                            start_milliseconds: value.start_milliseconds,
                            end_milliseconds: value.end_milliseconds,
                        })
                        .collect();
                    page.has_more
                }),
            TranscriptProjectionScope::Unsupported { .. } => {
                projection.summary = None;
                Ok(false)
            }
        };
        match result {
            Ok(has_more) => projection.has_more = has_more,
            Err(error) => {
                projection.speakers.clear();
                projection.segments.clear();
                projection.words.clear();
                projection.failure = Some(failure(storage_failure(error)));
            }
        }
        projection
    }
}

fn empty_projection(
    scope: TranscriptProjectionScope,
    operations: &[pod0_application::OperationProjection],
) -> TranscriptProjection {
    TranscriptProjection {
        scope,
        summary: None,
        speakers: Vec::new(),
        segments: Vec::new(),
        words: Vec::new(),
        operations: operations
            .iter()
            .take(pod0_application::MAX_OPERATION_ITEMS)
            .cloned()
            .collect(),
        failure: None,
        has_more: false,
    }
}

fn segment(value: pod0_storage::StoredTranscriptSegment) -> TranscriptSegmentProjection {
    TranscriptSegmentProjection {
        segment_id: value.segment_id,
        ordinal: value.ordinal,
        text: value.text,
        start_milliseconds: value.start_milliseconds,
        end_milliseconds: value.end_milliseconds,
        speaker_id: value.speaker_id,
        word_count: value.word_count,
    }
}
