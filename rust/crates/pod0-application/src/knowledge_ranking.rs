use std::collections::BTreeSet;

use pod0_domain::{EvidenceScoreComponents, RankedEvidenceReference};

use crate::{
    EvidenceCandidateObservation, EvidenceRankingError, MAX_RANK_CANDIDATES, MAX_RANKED_EVIDENCE,
};

const RRF_K: u64 = 60;
const RRF_SCALE: u64 = 1_000_000_000;

/// Combines raw capability ranks using integer reciprocal-rank fusion. Native
/// code supplies observations; this function owns policy and final ordering.
pub fn rank_evidence(
    candidates: &[EvidenceCandidateObservation],
    limit: u16,
) -> Result<Vec<RankedEvidenceReference>, EvidenceRankingError> {
    let requested = usize::from(limit);
    if requested == 0 {
        return Err(EvidenceRankingError::EmptyLimit);
    }
    if requested > MAX_RANKED_EVIDENCE {
        return Err(EvidenceRankingError::LimitTooLarge);
    }
    if candidates.len() > MAX_RANK_CANDIDATES {
        return Err(EvidenceRankingError::TooManyCandidates);
    }

    let mut span_ids = BTreeSet::new();
    let mut vector_ranks = BTreeSet::new();
    let mut lexical_ranks = BTreeSet::new();
    let mut ranked = Vec::with_capacity(candidates.len().min(requested));

    for candidate in candidates {
        if candidate.vector_rank.is_none() && candidate.lexical_rank.is_none() {
            return Err(EvidenceRankingError::CandidateHasNoRank {
                span_id: candidate.span_id,
            });
        }
        if !span_ids.insert(candidate.span_id) {
            return Err(EvidenceRankingError::DuplicateCandidate {
                span_id: candidate.span_id,
            });
        }
        let vector_rrf_units =
            validate_rank(candidate.vector_rank, &mut vector_ranks, RankLane::Vector)?;
        let lexical_rrf_units = validate_rank(
            candidate.lexical_rank,
            &mut lexical_ranks,
            RankLane::Lexical,
        )?;
        ranked.push(RankedEvidenceReference {
            span_id: candidate.span_id,
            score: EvidenceScoreComponents {
                vector_rrf_units,
                lexical_rrf_units,
                total_rrf_units: vector_rrf_units + lexical_rrf_units,
            },
        });
    }
    validate_rank_coverage(&vector_ranks, RankLane::Vector)?;
    validate_rank_coverage(&lexical_ranks, RankLane::Lexical)?;

    ranked.sort_by(|left, right| {
        right
            .score
            .total_rrf_units
            .cmp(&left.score.total_rrf_units)
            .then_with(|| left.span_id.cmp(&right.span_id))
    });
    ranked.truncate(requested);
    Ok(ranked)
}

fn validate_rank_coverage(
    ranks: &BTreeSet<u16>,
    lane: RankLane,
) -> Result<(), EvidenceRankingError> {
    if ranks
        .iter()
        .enumerate()
        .all(|(index, rank)| usize::from(*rank) == index + 1)
    {
        Ok(())
    } else {
        Err(match lane {
            RankLane::Vector => EvidenceRankingError::IncompleteVectorRanks,
            RankLane::Lexical => EvidenceRankingError::IncompleteLexicalRanks,
        })
    }
}

fn validate_rank(
    rank: Option<u16>,
    seen: &mut BTreeSet<u16>,
    lane: RankLane,
) -> Result<u64, EvidenceRankingError> {
    let Some(rank) = rank else { return Ok(0) };
    if rank == 0 || usize::from(rank) > MAX_RANK_CANDIDATES {
        return Err(match lane {
            RankLane::Vector => EvidenceRankingError::InvalidVectorRank { rank },
            RankLane::Lexical => EvidenceRankingError::InvalidLexicalRank { rank },
        });
    }
    if !seen.insert(rank) {
        return Err(match lane {
            RankLane::Vector => EvidenceRankingError::DuplicateVectorRank { rank },
            RankLane::Lexical => EvidenceRankingError::DuplicateLexicalRank { rank },
        });
    }
    Ok(RRF_SCALE / (RRF_K + u64::from(rank)))
}

#[derive(Clone, Copy)]
enum RankLane {
    Vector,
    Lexical,
}
