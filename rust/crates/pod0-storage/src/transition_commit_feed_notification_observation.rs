impl LibraryStore {
    pub fn commit_feed_notification_observation(
        &self,
        input: crate::feed_discovery_workflow_model::FeedNotificationObservationCommitInput,
    ) -> Result<
        crate::feed_discovery_workflow_model::FeedNotificationObservationCommitOutcome,
        StorageError,
    > {
        let identity = notification_observation_identity(
            input.lease.attempt_id,
            input.observation.sequence_number,
        );
        let fingerprint = notification_observation_fingerprint(&input.observation)?;
        let observed = input.observation.clone();
        let outcome = input.outcome;
        let receipt = TransitionCommit::open(self.path())?
            .commit_planned_with_transaction_hooks(
                TransitionIngress {
                    kind: TransitionIngressKind::HostObservation,
                    id: identity.into_bytes(),
                    fingerprint,
                },
                input.committed_at,
                |transaction| {
                    let current = crate::feed_discovery_workflow_store::effect_for_request(
                        transaction,
                        observed.request_id,
                    )?
                    .ok_or(StorageError::EntityNotFound)?;
                    let command_id = current.command_id.ok_or(StorageError::InvalidActivity)?;
                    let revision = current_library_revision(transaction)?;
                    let retry_effect =
                        notification_retry_effect(&current, outcome, revision, &observed)?;
                    pod0_application::plan_feed_notification_observation(
                        pod0_application::FeedNotificationObservationInput {
                            identity_attempt_id: identity,
                            effect_attempt_id: input.lease.attempt_id,
                            intent_id: input.lease.intent_id,
                            authorizing_activity_id: input.lease.authorizing_activity_id,
                            correlation_id: input.lease.correlation_id,
                            command_id,
                            request_id: observed.request_id,
                            episode_id: current.episode_id,
                            current_revision: revision,
                            outcome: notification_effect_outcome(outcome),
                            retry_effect,
                        },
                    )
                    .map_err(|_| StorageError::InvalidActivity)
                },
                |transaction| stage_notification_observation(transaction, input.lease, &observed),
                |transaction, expected, mutation| {
                    if mutation != pod0_application::FeedNotificationObservationMutation::Apply
                        || current_library_revision(transaction)? != expected
                    {
                        return Err(StorageError::RevisionConflict);
                    }
                    let current = crate::feed_discovery_workflow_store::effect_for_request(
                        transaction,
                        observed.request_id,
                    )?
                    .ok_or(StorageError::EntityNotFound)?;
                    let revision = crate::library_store::advance_playback_revision(transaction)?;
                    apply_notification_observation(
                        transaction,
                        &current,
                        outcome,
                        observed.observed_at.value,
                        revision,
                    )?;
                    Ok(revision)
                },
                |transaction| complete_feed_observation(transaction, input.lease),
            )
            .map_err(|error| match error {
                StorageError::ActivityCommandConflict => StorageError::CommandConflict,
                other => other,
            })?;
        Ok(
            crate::feed_discovery_workflow_model::FeedNotificationObservationCommitOutcome {
                replayed: receipt.replayed,
                effect: None,
            },
        )
    }
}

fn notification_retry_effect(
    current: &FeedDiscoveryEffectRecord,
    outcome: FeedDiscoveryNotificationOutcome,
    revision: StateRevision,
    observation: &pod0_application::DurableFeedHostObservation,
) -> Result<Option<pod0_application::DurableFeedEffectRequest>, StorageError> {
    if outcome != FeedDiscoveryNotificationOutcome::RetryableFailure
        || current.attempt >= FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS
        || observation.observed_at.value >= current.expires_at_ms
    {
        return Ok(None);
    }
    let attempt = current
        .attempt
        .checked_add(1)
        .ok_or(StorageError::InvalidActivity)?;
    let request_id = pod0_application::feed_discovery_notification_request_id(
        current.occurrence_id,
        current.episode_id,
        attempt,
    );
    Ok(Some(pod0_application::DurableFeedEffectRequest {
        request_id,
        command_id: CommandId::from_bytes(request_id.into_bytes()),
        cancellation_id: current.cancellation_id,
        issued_revision: StateRevision::new(
            revision
                .value
                .checked_add(1)
                .ok_or(StorageError::InvalidActivity)?,
        ),
        not_before: Some(UnixTimestampMilliseconds::new(
            observation
                .observed_at
                .value
                .saturating_add(pod0_application::FEED_DISCOVERY_NOTIFICATION_RETRY_MILLISECONDS),
        )),
        deadline_at: Some(UnixTimestampMilliseconds::new(current.expires_at_ms)),
        action: pod0_application::DurableFeedEffectAction::DeliverNewEpisodeNotification {
            occurrence_id: current.occurrence_id,
            episode_id: current.episode_id,
            podcast_id: current.podcast_id,
            podcast_title: current.podcast_title.clone(),
            episode_title: current.episode_title.clone(),
        },
    }))
}

fn apply_notification_observation(
    transaction: &Transaction<'_>,
    current: &FeedDiscoveryEffectRecord,
    outcome: FeedDiscoveryNotificationOutcome,
    now_ms: i64,
    revision: StateRevision,
) -> Result<(), StorageError> {
    if outcome == FeedDiscoveryNotificationOutcome::RetryableFailure
        && current.attempt < FEED_DISCOVERY_NOTIFICATION_MAX_ATTEMPTS
        && now_ms < current.expires_at_ms
    {
        let attempt = current
            .attempt
            .checked_add(1)
            .ok_or(StorageError::InvalidActivity)?;
        let request_id = pod0_application::feed_discovery_notification_request_id(
            current.occurrence_id,
            current.episode_id,
            attempt,
        );
        let changed = transaction
            .execute(
                "UPDATE pod0_feed_discovery_effects SET stage='requested',attempt=?1,request_id=?2,\
             not_before_ms=?3,deadline_at_ms=?4,failure_code=NULL,updated_at_ms=?5 WHERE \
             occurrence_id=?6 AND episode_id=?7 AND kind='notification' AND request_id=?8",
                params![
                    i64::from(attempt),
                    request_id.into_bytes().as_slice(),
                    now_ms.saturating_add(
                        pod0_application::FEED_DISCOVERY_NOTIFICATION_RETRY_MILLISECONDS
                    ),
                    current.expires_at_ms,
                    now_ms,
                    current.occurrence_id.into_bytes().as_slice(),
                    current.episode_id.into_bytes().as_slice(),
                    current
                        .request_id
                        .ok_or(StorageError::InvalidActivity)?
                        .into_bytes()
                        .as_slice()
                ],
            )
            .map_err(|error| StorageError::sqlite("authorize notification retry", error))?;
        if changed != 1 {
            return Err(StorageError::RevisionConflict);
        }
        transaction
            .execute(
                "UPDATE pod0_feed_discovery_workflows SET workflow_revision=?1,updated_at_ms=?2 \
             WHERE occurrence_id=?3",
                params![
                    i64::try_from(revision.value).map_err(|_| StorageError::InvalidActivity)?,
                    now_ms,
                    current.occurrence_id.into_bytes().as_slice()
                ],
            )
            .map_err(|error| StorageError::sqlite("advance notification retry workflow", error))?;
        return Ok(());
    }
    crate::feed_discovery_workflow_store::finish_notification_in_transaction(
        transaction,
        current.request_id.ok_or(StorageError::InvalidActivity)?,
        outcome,
        now_ms,
    )?;
    Ok(())
}

fn stage_notification_observation(
    transaction: &Transaction<'_>,
    lease: pod0_application::PersistedEffectLeaseIdentity,
    observation: &pod0_application::DurableFeedHostObservation,
) -> Result<(), StorageError> {
    let fence = i64::try_from(lease.fence).map_err(|_| StorageError::InvalidActivity)?;
    let payload = serde_json::to_string(observation).map_err(|_| StorageError::InvalidActivity)?;
    let state: Option<(i64, Option<String>, String)> = transaction
        .query_row(
            "SELECT a.state_code,a.observation_json,i.request_json FROM pod0_effect_attempts a \
         JOIN pod0_effect_intents i ON i.intent_id=a.intent_id JOIN pod0_feed_discovery_effects e \
         ON e.episode_id=i.subject_id AND e.request_id=?1 WHERE a.lease_id=?2 AND a.fence=?3 \
         AND a.attempt_id=?4 AND a.intent_id=?5 AND i.authorizing_activity_id=?6 \
         AND i.correlation_id=?7 AND a.lease_expires_at_ms>=?8 AND a.lease_expires_at_ms=?9 \
         AND i.effect_kind_code=6 AND i.subject_code=2 AND e.cancellation_id=?10",
            params![
                observation.request_id.into_bytes().as_slice(),
                lease.lease_id.into_bytes().as_slice(),
                fence,
                lease.attempt_id.into_bytes().as_slice(),
                lease.intent_id.into_bytes().as_slice(),
                lease.authorizing_activity_id.into_bytes().as_slice(),
                lease.correlation_id.into_bytes().as_slice(),
                observation.observed_at.value,
                lease.expires_at.value,
                observation.cancellation_id.into_bytes().as_slice()
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("validate notification observation lease", error))?;
    let Some((state_code, stored, request_json)) = state else {
        return Err(StorageError::CommandConflict);
    };
    if state_code == 2 && stored.as_deref() == Some(&payload) {
        return Ok(());
    }
    let request: pod0_application::DurableExternalEffectRequest =
        serde_json::from_str(&request_json).map_err(|_| StorageError::InvalidActivity)?;
    let pod0_application::DurableEffectExecution::Feed { request } = request.execution else {
        return Err(StorageError::CommandConflict);
    };
    if state_code != 1
        || request.request_id != observation.request_id
        || request.cancellation_id != observation.cancellation_id
        || request.issued_revision != observation.observed_request_revision
    {
        return Err(StorageError::CommandConflict);
    }
    let outcome_json = serde_json::to_string(&pod0_application::EffectOutcome::Succeeded)
        .map_err(|_| StorageError::InvalidActivity)?;
    let changed = transaction.execute(
        "UPDATE pod0_effect_attempts SET state_code=2,observed_at_ms=?1,outcome_schema_version=1,\
         outcome_json=?2,observation_schema_version=1,observation_json=?3 WHERE lease_id=?4 \
         AND fence=?5 AND state_code=1",
        params![observation.observed_at.value,outcome_json,payload,
            lease.lease_id.into_bytes().as_slice(),fence],
    ).map_err(|error| StorageError::sqlite("stage notification observation", error))?;
    (changed == 1)
        .then_some(())
        .ok_or(StorageError::CommandConflict)
}

fn notification_effect_outcome(
    outcome: FeedDiscoveryNotificationOutcome,
) -> pod0_application::EffectOutcome {
    match outcome {
        FeedDiscoveryNotificationOutcome::Delivered => pod0_application::EffectOutcome::Succeeded,
        FeedDiscoveryNotificationOutcome::Cancelled => pod0_application::EffectOutcome::Cancelled,
        FeedDiscoveryNotificationOutcome::PermissionDenied => {
            pod0_application::EffectOutcome::Failed {
                code: pod0_application::ActivityFailureCode::PermissionDenied,
            }
        }
        FeedDiscoveryNotificationOutcome::RetryableFailure
        | FeedDiscoveryNotificationOutcome::PermanentFailure => {
            pod0_application::EffectOutcome::Failed {
                code: pod0_application::ActivityFailureCode::PlatformFailure,
            }
        }
    }
}
