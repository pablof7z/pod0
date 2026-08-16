use pod0_application::{FeedNotificationAdmissionInput, plan_feed_notification_admission};

impl LibraryStore {
    pub(crate) fn commit_feed_notification_admission(
        &self,
        occurrence_id: FeedDiscoveryOccurrenceId,
        episode_id: EpisodeId,
        now_ms: i64,
        deadline_at_ms: i64,
    ) -> Result<Option<FeedDiscoveryEffectRecord>, StorageError> {
        let fingerprint =
            notification_admission_fingerprint(occurrence_id, episode_id, now_ms, deadline_at_ms);
        let receipt = TransitionCommit::open(self.path())?.commit_planned_with(
            TransitionIngress {
                kind: TransitionIngressKind::Recovery,
                id: notification_admission_id(occurrence_id, episode_id, now_ms),
                fingerprint,
            },
            UnixTimestampMilliseconds::new(now_ms),
            |transaction| {
                let record = crate::feed_discovery_workflow_store::read_effect(
                    transaction,
                    occurrence_id,
                    episode_id,
                    FeedDiscoveryEffectKind::Notification,
                )?
                .ok_or(StorageError::EntityNotFound)?;
                if !matches!(
                    record.stage,
                    FeedDiscoveryEffectStage::Pending
                        | FeedDiscoveryEffectStage::RetryScheduled
                ) || record.not_before_ms.is_some_and(|value| value > now_ms)
                {
                    return Err(StorageError::CommandConflict);
                }
                let attempt = record
                    .attempt
                    .checked_add(1)
                    .filter(|value| *value <= FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS)
                    .ok_or(StorageError::CommandConflict)?;
                let request_id = pod0_application::feed_discovery_notification_request_id(
                    occurrence_id,
                    episode_id,
                    attempt,
                );
                let command_id = CommandId::from_bytes(request_id.into_bytes());
                let current = current_library_revision(transaction)?;
                let effect = DurableFeedEffectRequest {
                    request_id,
                    command_id,
                    cancellation_id: record.cancellation_id,
                    issued_revision: StateRevision::new(
                        current
                            .value
                            .checked_add(1)
                            .ok_or(StorageError::InvalidActivity)?,
                    ),
                    not_before: record.not_before_ms.map(UnixTimestampMilliseconds::new),
                    deadline_at: Some(UnixTimestampMilliseconds::new(deadline_at_ms)),
                    action: DurableFeedEffectAction::DeliverNewEpisodeNotification {
                        occurrence_id,
                        episode_id,
                        podcast_id: record.podcast_id,
                        podcast_title: record.podcast_title,
                        episode_title: record.episode_title,
                    },
                };
                plan_feed_notification_admission(FeedNotificationAdmissionInput {
                    command_id,
                    episode_id,
                    current_revision: current,
                    effect,
                })
                .map(|plan| {
                    plan.map_mutation(|mutation: ()| {
                        let _ = mutation;
                        (attempt, request_id)
                    })
                })
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction, expected, (attempt, request_id)| {
                require_revision(transaction, expected)?;
                let committed = expected
                    .value
                    .checked_add(1)
                    .ok_or(StorageError::InvalidActivity)?;
                let changed = transaction.execute(
                    "UPDATE pod0_feed_discovery_effects SET stage='requested',attempt=?1,\
                     request_id=?2,not_before_ms=NULL,deadline_at_ms=?3,failure_code=NULL,\
                     updated_at_ms=?4 WHERE occurrence_id=?5 AND episode_id=?6 \
                     AND kind='notification' AND stage IN('pending','retry_scheduled')",
                    params![i64::from(attempt), request_id.into_bytes().as_slice(), deadline_at_ms,
                        now_ms, occurrence_id.into_bytes().as_slice(), episode_id.into_bytes().as_slice()],
                ).map_err(|error| StorageError::sqlite("admit durable feed notification", error))?;
                if changed != 1 { return Err(StorageError::RevisionConflict); }
                transaction.execute(
                    "UPDATE pod0_feed_discovery_workflows SET workflow_revision=?1,updated_at_ms=?2 \
                     WHERE occurrence_id=?3",
                    params![i64::try_from(committed).map_err(|_| StorageError::InvalidActivity)?,
                        now_ms, occurrence_id.into_bytes().as_slice()],
                ).map_err(|error| StorageError::sqlite("advance notification workflow", error))?;
                crate::library_store::advance_playback_revision(transaction)
            },
        )?;
        let _ = receipt;
        self.requested_feed_discovery_notifications(64)
            .map(|records| {
                records.into_iter().find(|record| {
                    record.occurrence_id == occurrence_id && record.episode_id == episode_id
                })
            })
    }
}

fn notification_admission_id(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
    now_ms: i64,
) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update(b"pod0-feed-notification-admission-id-v1\0");
    hash.update(occurrence_id.into_bytes());
    hash.update(episode_id.into_bytes());
    hash.update(now_ms.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    digest[..16].try_into().expect("sha256 prefix is 16 bytes")
}

fn notification_admission_fingerprint(
    occurrence_id: FeedDiscoveryOccurrenceId,
    episode_id: EpisodeId,
    now_ms: i64,
    deadline_at_ms: i64,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0-feed-notification-admission-v1\0");
    hash.update(occurrence_id.into_bytes());
    hash.update(episode_id.into_bytes());
    hash.update(now_ms.to_be_bytes());
    hash.update(deadline_at_ms.to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}
