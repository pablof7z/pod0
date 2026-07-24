use std::collections::{BTreeMap, BTreeSet};

use crate::{
    LegacyFeedDiscoveryCutoverInput, LegacyFeedDiscoveryDisposition, LegacyFeedDiscoveryEffectKind,
    MAX_LEGACY_FEED_DISCOVERY_CANDIDATES, StorageError,
};

pub(super) fn validate_input(input: &LegacyFeedDiscoveryCutoverInput) -> Result<(), StorageError> {
    if input.candidates.len() > MAX_LEGACY_FEED_DISCOVERY_CANDIDATES
        || (input.inspected_job_count == 0
            && (!input.candidates.is_empty() || input.blocked_count != 0))
        || input.ambiguous_count as usize
            != input
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.disposition == LegacyFeedDiscoveryDisposition::Ambiguous
                })
                .count()
    {
        return Err(StorageError::InvalidFeedDiscoveryCutover);
    }
    let mut identities = BTreeSet::new();
    let mut occurrences = BTreeMap::new();
    let mut items = BTreeMap::new();
    for candidate in &input.candidates {
        validate_candidate(candidate, input.observed_at.value())?;
        let identity = (
            candidate.occurrence_id.into_bytes(),
            candidate.episode_id.into_bytes(),
            candidate.kind.wire(),
        );
        if !identities.insert(identity) {
            return Err(StorageError::InvalidFeedDiscoveryCutover);
        }
        let occurrence = (
            candidate.command_id.into_bytes(),
            candidate.podcast_id.into_bytes(),
            candidate.observed_at.value(),
            candidate.expires_at.value(),
        );
        if let Some(existing) = occurrences.insert(candidate.occurrence_id.into_bytes(), occurrence)
            && existing != occurrence
        {
            return Err(StorageError::InvalidFeedDiscoveryCutover);
        }
        let item = (
            candidate.published_at.value(),
            candidate.input_version.as_str(),
        );
        if let Some(existing) = items.insert(
            (
                candidate.occurrence_id.into_bytes(),
                candidate.episode_id.into_bytes(),
            ),
            item,
        ) && existing != item
        {
            return Err(StorageError::InvalidFeedDiscoveryCutover);
        }
    }
    Ok(())
}

fn validate_candidate(
    candidate: &crate::LegacyFeedDiscoveryCandidate,
    cutover_at_ms: i64,
) -> Result<(), StorageError> {
    if candidate.occurrence_id
        != pod0_application::feed_discovery_occurrence_id(candidate.command_id)
        || candidate.attempt > 4
        || candidate.observed_at.value() < 0
        || candidate.expires_at.value() < candidate.observed_at.value()
        || candidate.input_version.len() != 64
        || !candidate
            .input_version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StorageError::InvalidFeedDiscoveryCutover);
    }
    if candidate.disposition == LegacyFeedDiscoveryDisposition::Pending
        && candidate.expires_at.value() <= cutover_at_ms
    {
        return Err(StorageError::InvalidFeedDiscoveryCutover);
    }
    if candidate.disposition != LegacyFeedDiscoveryDisposition::Pending
        && candidate.not_before.is_some()
    {
        return Err(StorageError::InvalidFeedDiscoveryCutover);
    }
    if candidate.disposition == LegacyFeedDiscoveryDisposition::Ambiguous
        && candidate.kind != LegacyFeedDiscoveryEffectKind::Notification
    {
        return Err(StorageError::InvalidFeedDiscoveryCutover);
    }
    Ok(())
}
