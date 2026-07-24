use pod0_domain::{ContentDigest, UnixTimestampMilliseconds};
use pod0_storage::{
    LegacyFeedDiscoveryCandidate, LegacyFeedDiscoveryCutoverInput, LegacyFeedDiscoveryDisposition,
    LegacyFeedDiscoveryEffectKind, StorageError,
};

use crate::{
    LegacyFeedDiscoveryCandidateInput, LegacyFeedDiscoveryDispositionInput,
    LegacyFeedDiscoveryEffectKindInput,
};

pub(super) fn cutover_input(
    backup_digest: ContentDigest,
    backup_byte_count: u64,
    notifications_enabled: bool,
    inspected_job_count: u32,
    blocked_count: u32,
    candidates: Vec<LegacyFeedDiscoveryCandidateInput>,
    observed_at: UnixTimestampMilliseconds,
) -> Result<LegacyFeedDiscoveryCutoverInput, StorageError> {
    let candidates = candidates
        .into_iter()
        .map(|candidate| map_candidate(backup_digest, candidate))
        .collect::<Result<Vec<_>, StorageError>>()?;
    let ambiguous_count = u32::try_from(
        candidates
            .iter()
            .filter(|candidate| candidate.disposition == LegacyFeedDiscoveryDisposition::Ambiguous)
            .count(),
    )
    .map_err(|_| StorageError::InvalidFeedDiscoveryCutover)?;
    Ok(LegacyFeedDiscoveryCutoverInput {
        backup_digest,
        backup_byte_count,
        notification_command_id: pod0_application::legacy_feed_discovery_notification_command_id(
            backup_digest,
        ),
        notifications_enabled,
        inspected_job_count,
        blocked_count,
        ambiguous_count,
        candidates,
        observed_at,
    })
}

fn map_candidate(
    backup_digest: ContentDigest,
    input: LegacyFeedDiscoveryCandidateInput,
) -> Result<LegacyFeedDiscoveryCandidate, StorageError> {
    let command_id = pod0_application::legacy_feed_discovery_command_id(
        backup_digest,
        input.source_occurrence_id,
    );
    let occurrence_id = pod0_application::feed_discovery_occurrence_id(command_id);
    let (disposition, attempt, not_before) = disposition(input.disposition);
    Ok(LegacyFeedDiscoveryCandidate {
        occurrence_id,
        command_id,
        podcast_id: input.podcast_id,
        episode_id: input.episode_id,
        kind: match input.kind {
            LegacyFeedDiscoveryEffectKindInput::Download => LegacyFeedDiscoveryEffectKind::Download,
            LegacyFeedDiscoveryEffectKindInput::Notification => {
                LegacyFeedDiscoveryEffectKind::Notification
            }
        },
        disposition,
        attempt,
        not_before,
        observed_at: input.observed_at,
        expires_at: input.expires_at,
        published_at: input.published_at,
        input_version: input.input_version,
    })
}

fn disposition(
    value: LegacyFeedDiscoveryDispositionInput,
) -> (
    LegacyFeedDiscoveryDisposition,
    u8,
    Option<UnixTimestampMilliseconds>,
) {
    match value {
        LegacyFeedDiscoveryDispositionInput::Pending {
            attempt,
            not_before,
        } => (LegacyFeedDiscoveryDisposition::Pending, attempt, not_before),
        LegacyFeedDiscoveryDispositionInput::Succeeded { attempt } => {
            (LegacyFeedDiscoveryDisposition::Succeeded, attempt, None)
        }
        LegacyFeedDiscoveryDispositionInput::Obsolete { attempt } => {
            (LegacyFeedDiscoveryDisposition::Obsolete, attempt, None)
        }
        LegacyFeedDiscoveryDispositionInput::Failed { attempt } => {
            (LegacyFeedDiscoveryDisposition::Failed, attempt, None)
        }
        LegacyFeedDiscoveryDispositionInput::Ambiguous { attempt } => {
            (LegacyFeedDiscoveryDisposition::Ambiguous, attempt, None)
        }
    }
}
