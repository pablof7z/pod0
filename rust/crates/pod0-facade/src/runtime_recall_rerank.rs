use std::collections::{BTreeMap, BTreeSet};

use pod0_application::{MAX_RECALL_EVIDENCE, RecallEvidenceProjection, RecallRerankObservation};
use pod0_domain::EvidenceSpanId;

pub(super) fn validate_rerank(
    evidence: &[RecallEvidenceProjection],
    rankings: &[RecallRerankObservation],
) -> Option<BTreeMap<EvidenceSpanId, u16>> {
    if evidence.len() != rankings.len() || evidence.len() > MAX_RECALL_EVIDENCE {
        return None;
    }
    let expected = evidence
        .iter()
        .map(|item| item.span_id)
        .collect::<BTreeSet<_>>();
    let observed = rankings
        .iter()
        .map(|item| item.span_id)
        .collect::<BTreeSet<_>>();
    let ranks = rankings
        .iter()
        .map(|item| item.rank)
        .collect::<BTreeSet<_>>();
    if expected != observed
        || ranks.len() != rankings.len()
        || !ranks
            .iter()
            .enumerate()
            .all(|(index, rank)| usize::from(*rank) == index + 1)
    {
        return None;
    }
    Some(
        rankings
            .iter()
            .map(|item| (item.span_id, item.rank))
            .collect(),
    )
}
