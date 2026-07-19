use pod0_application::{
    EvidenceIndexProjection, EvidenceIndexSpanProjection, EvidenceIndexStage,
    MAX_EVIDENCE_INDEX_PAGE_ITEMS,
};
use pod0_domain::EpisodeId;

use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn evidence_index_projection(
        &self,
        episode_id: EpisodeId,
        offset: usize,
        requested_items: usize,
    ) -> EvidenceIndexProjection {
        let unavailable = || EvidenceIndexProjection {
            episode_id,
            stage: EvidenceIndexStage::Unavailable,
            generation_id: None,
            transcript_content_digest: None,
            spans: Vec::new(),
            total_spans: 0,
            has_more: false,
        };
        let Some(store) = &self.evidence_store else {
            return unavailable();
        };
        let artifact = match store.selected_artifact(episode_id) {
            Ok(Some(artifact)) => artifact,
            Ok(None) => {
                return EvidenceIndexProjection {
                    episode_id,
                    stage: EvidenceIndexStage::Missing,
                    generation_id: None,
                    transcript_content_digest: None,
                    spans: Vec::new(),
                    total_spans: 0,
                    has_more: false,
                };
            }
            Err(_) => return unavailable(),
        };
        let total = artifact.spans.len();
        let limit = requested_items.clamp(1, MAX_EVIDENCE_INDEX_PAGE_ITEMS);
        let spans = artifact
            .spans
            .iter()
            .skip(offset)
            .take(limit)
            .map(|span| EvidenceIndexSpanProjection {
                span_id: span.span_id,
                generation_id: artifact.generation_id,
                episode_id: span.episode_id,
                podcast_id: span.podcast_id,
                text: span.text.clone(),
            })
            .collect::<Vec<_>>();
        EvidenceIndexProjection {
            episode_id,
            stage: EvidenceIndexStage::Ready,
            generation_id: Some(artifact.generation_id),
            transcript_content_digest: Some(artifact.version.content_digest),
            has_more: total > offset.saturating_add(spans.len()),
            total_spans: u32::try_from(total).unwrap_or(u32::MAX),
            spans,
        }
    }
}
