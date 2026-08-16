fn effect_stage(
    disposition: LegacyFeedDiscoveryDisposition,
) -> (&'static str, Option<&'static str>) {
    match disposition {
        LegacyFeedDiscoveryDisposition::Pending => ("pending", None),
        LegacyFeedDiscoveryDisposition::Succeeded => ("succeeded", None),
        LegacyFeedDiscoveryDisposition::Obsolete => ("obsolete", Some("legacy_obsolete")),
        LegacyFeedDiscoveryDisposition::Failed => ("failed", Some("legacy_failed")),
        LegacyFeedDiscoveryDisposition::Ambiguous => {
            ("succeeded", Some("legacy_ambiguous_delivery"))
        }
    }
}

fn effect_identity(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
    kind: LegacyFeedDiscoveryEffectKind,
) -> (Option<pod0_domain::CommandId>, pod0_domain::CancellationId) {
    match kind {
        LegacyFeedDiscoveryEffectKind::Download => (
            Some(pod0_application::feed_discovery_download_command_id(
                occurrence_id,
                episode_id,
            )),
            pod0_application::feed_discovery_download_cancellation_id(occurrence_id, episode_id),
        ),
        LegacyFeedDiscoveryEffectKind::Notification => (
            None,
            pod0_application::feed_discovery_notification_cancellation_id(
                occurrence_id,
                episode_id,
            ),
        ),
    }
}

fn occurrence_fingerprint(candidate: &LegacyFeedDiscoveryCandidate) -> String {
    let mut hash = Sha256::new();
    hash.update(b"pod0-legacy-feed-discovery-occurrence-v1");
    hash.update(candidate.occurrence_id.into_bytes());
    hash.update(candidate.command_id.into_bytes());
    digest_hex(hash.finalize().into())
}

fn digest_hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
