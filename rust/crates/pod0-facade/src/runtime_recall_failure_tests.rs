use crate::runtime_recall_test_support::{RecallFixture, advance_to_rerank, record};
use crate::*;

#[test]
fn malformed_query_embedding_fails_closed_and_optional_rerank_falls_back() {
    let malformed = RecallFixture::new(true);
    malformed.dispatch(30, 30, "habit cues");
    let embed = malformed
        .base
        .facade
        .next_leased_host_requests(1)
        .pop()
        .unwrap();
    record(
        &malformed.base.facade,
        &embed,
        HostObservation::RecallQueryEmbedded {
            query_id: RecallQueryId::from_parts(32, 30),
            embedding: RecallEmbeddingVector {
                values: vec![10, -10],
            },
        },
    );
    assert_eq!(
        malformed.projection(30).stage,
        RecallStage::IndexUnavailable
    );
    assert!(
        malformed
            .base
            .facade
            .next_leased_host_requests(1)
            .is_empty()
    );

    let fallback = RecallFixture::new(true);
    fallback.dispatch(31, 31, "habit cues");
    advance_to_rerank(&fallback, 31);
    let rerank = fallback
        .base
        .facade
        .next_leased_host_requests(1)
        .pop()
        .unwrap();
    record(
        &fallback.base.facade,
        &rerank,
        HostObservation::Failed {
            code: HostFailureCode::ProviderUnavailable,
            safe_detail: None,
        },
    );
    let projection = fallback.projection(31);
    assert_eq!(projection.stage, RecallStage::Ready);
    assert!(
        projection
            .evidence
            .iter()
            .all(|item| item.score.rerank_rank.is_none())
    );
}
