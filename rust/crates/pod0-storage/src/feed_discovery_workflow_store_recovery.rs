fn workflow_for_occurrence(
    connection: &Connection,
    occurrence_id: FeedDiscoveryOccurrenceId,
) -> Result<Option<()>, StorageError> {
    connection
        .query_row(
            "SELECT 1 FROM pod0_feed_discovery_workflows WHERE occurrence_id=?1",
            [occurrence_id.into_bytes().as_slice()],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read feed-discovery workflow existence", error))
}

pub(crate) fn feed_discovery_recovery_identity(
    phase: &[u8],
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: Option<EpisodeId>,
    state_revision: StateRevision,
    decision_variant: bool,
) -> (CommandId, pod0_domain::ContentDigest) {
    let mut identity = Sha256::new();
    identity.update(b"pod0-feed-discovery-recovery-id-v1\0");
    identity.update(phase);
    identity.update(occurrence_id.into_bytes());
    if let Some(episode_id) = episode_id {
        identity.update(episode_id.into_bytes());
    }
    // A state-versioned identity makes the same recovery decision replay
    // exactly across restarts, while a later authoritative state can admit a
    // genuinely new reconciliation. Wall-clock polling must not manufacture
    // an unbounded stream of no-op activity facts.
    identity.update(state_revision.value.to_be_bytes());
    identity.update([u8::from(decision_variant)]);
    let identity: [u8; 32] = identity.finalize().into();
    let mut fingerprint = Sha256::new();
    fingerprint.update(b"pod0-feed-discovery-recovery-fingerprint-v1\0");
    fingerprint.update(phase);
    fingerprint.update(occurrence_id.into_bytes());
    if let Some(episode_id) = episode_id {
        fingerprint.update(episode_id.into_bytes());
    }
    fingerprint.update(state_revision.value.to_be_bytes());
    fingerprint.update([u8::from(decision_variant)]);
    (
        CommandId::from_bytes(identity[..16].try_into().expect("digest prefix")),
        pod0_domain::ContentDigest::from_bytes(fingerprint.finalize().into()),
    )
}
