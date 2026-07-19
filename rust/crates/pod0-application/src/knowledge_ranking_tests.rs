use pod0_domain::EvidenceSpanId;

use crate::{
    EvidenceCandidateObservation, EvidenceRankingError, MAX_RANK_CANDIDATES, MAX_RANKED_EVIDENCE,
    rank_evidence,
};

fn candidate(
    id: u64,
    vector_rank: Option<u16>,
    lexical_rank: Option<u16>,
) -> EvidenceCandidateObservation {
    EvidenceCandidateObservation {
        span_id: EvidenceSpanId::from_parts(0, id),
        vector_rank,
        lexical_rank,
    }
}

#[test]
fn integer_rrf_is_stable_and_preserves_score_components() {
    let candidates = [
        candidate(1, Some(1), Some(3)),
        candidate(2, Some(2), Some(1)),
        candidate(3, Some(3), Some(2)),
    ];
    let first = rank_evidence(&candidates, 3).unwrap();
    let second = rank_evidence(&candidates, 3).unwrap();

    assert_eq!(first, second);
    assert_eq!(first[0].span_id, EvidenceSpanId::from_parts(0, 2));
    assert_eq!(first[0].score.vector_rrf_units, 1_000_000_000 / 62);
    assert_eq!(first[0].score.lexical_rrf_units, 1_000_000_000 / 61);
    assert_eq!(
        first[0].score.total_rrf_units,
        first[0].score.vector_rrf_units + first[0].score.lexical_rrf_units
    );
}

#[test]
fn tied_scores_use_stable_span_identity_and_limit_is_bounded() {
    let candidates = [candidate(2, Some(1), None), candidate(1, None, Some(1))];
    let ranked = rank_evidence(&candidates, 1).unwrap();
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].span_id, EvidenceSpanId::from_parts(0, 1));
}

#[test]
fn malformed_candidate_sets_fail_closed() {
    assert_eq!(
        rank_evidence(&[candidate(1, None, None)], 1),
        Err(EvidenceRankingError::CandidateHasNoRank {
            span_id: EvidenceSpanId::from_parts(0, 1)
        })
    );
    assert_eq!(
        rank_evidence(&[candidate(1, Some(0), None)], 1),
        Err(EvidenceRankingError::InvalidVectorRank { rank: 0 })
    );
    assert_eq!(
        rank_evidence(
            &[candidate(
                1,
                None,
                Some(u16::try_from(MAX_RANK_CANDIDATES + 1).unwrap())
            )],
            1
        ),
        Err(EvidenceRankingError::InvalidLexicalRank {
            rank: u16::try_from(MAX_RANK_CANDIDATES + 1).unwrap()
        })
    );
    assert_eq!(
        rank_evidence(
            &[candidate(1, Some(1), None), candidate(1, Some(2), None)],
            1
        ),
        Err(EvidenceRankingError::DuplicateCandidate {
            span_id: EvidenceSpanId::from_parts(0, 1)
        })
    );
    assert_eq!(
        rank_evidence(
            &[candidate(1, Some(1), None), candidate(2, Some(1), None)],
            1
        ),
        Err(EvidenceRankingError::DuplicateVectorRank { rank: 1 })
    );
    assert_eq!(
        rank_evidence(
            &[candidate(1, Some(1), None), candidate(2, Some(3), None)],
            1
        ),
        Err(EvidenceRankingError::IncompleteVectorRanks)
    );
    assert_eq!(
        rank_evidence(&[candidate(1, Some(1), None)], 0),
        Err(EvidenceRankingError::EmptyLimit)
    );
    assert_eq!(
        rank_evidence(
            &[candidate(1, Some(1), None)],
            u16::try_from(MAX_RANKED_EVIDENCE + 1).unwrap()
        ),
        Err(EvidenceRankingError::LimitTooLarge)
    );
    assert_eq!(
        rank_evidence(
            &vec![candidate(1, Some(1), None); MAX_RANK_CANDIDATES + 1],
            1
        ),
        Err(EvidenceRankingError::TooManyCandidates)
    );
}
