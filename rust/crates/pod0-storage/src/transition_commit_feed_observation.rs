use pod0_application::{
    ActivityFailureCode, EffectOutcome, FeedObservationActivityInput, FeedObservationMutation,
    plan_feed_observation,
};
use sha2::{Digest, Sha256};

use crate::feed_fetch_store_model::{
    FeedFetchLeasedObservationAction, FeedFetchObservationCommitInput,
    FeedFetchObservationCommitOutcome, FeedFetchWorkflowRecord,
};

impl LibraryStore {
    pub fn commit_feed_fetch_observation(
        &self,
        input: FeedFetchObservationCommitInput,
    ) -> Result<FeedFetchObservationCommitOutcome, StorageError> {
        let identity =
            feed_observation_identity(input.lease.attempt_id, input.observation.sequence_number);
        let fingerprint = feed_observation_fingerprint(&input)?;
        let observed = input.observation.clone();
        let action = input.action.clone();
        let planned_action = action.clone();
        let applied_action = action.clone();
        let effect_outcome = feed_effect_outcome(&action);
        let receipt = TransitionCommit::open(self.path())?
            .commit_planned_with_transaction_hooks(
                TransitionIngress {
                    kind: TransitionIngressKind::HostObservation,
                    id: identity.into_bytes(),
                    fingerprint,
                },
                input.committed_at,
                |transaction| {
                    let current = workflow_for_request(transaction, observed.request_id)?
                        .ok_or(StorageError::EntityNotFound)?;
                    let revision = current_library_revision(transaction)?;
                    let next_effect = retry_effect(&current, &planned_action, revision)?;
                    plan_feed_observation(FeedObservationActivityInput {
                        identity_attempt_id: identity,
                        effect_attempt_id: input.lease.attempt_id,
                        intent_id: input.lease.intent_id,
                        authorizing_activity_id: input.lease.authorizing_activity_id,
                        correlation_id: input.lease.correlation_id,
                        command_id: current.command_id,
                        request_id: current.request_id,
                        podcast_id: current.podcast_id,
                        current_revision: revision,
                        outcome: effect_outcome,
                        state_changes: true,
                        next_effect,
                    })
                    .map_err(|_| StorageError::InvalidActivity)
                },
                |transaction| stage_feed_observation(transaction, input.lease, &observed),
                |transaction, expected, mutation| {
                    if mutation != FeedObservationMutation::Apply
                        || current_library_revision(transaction)? != expected
                    {
                        return Err(StorageError::RevisionConflict);
                    }
                    let current = workflow_for_request(transaction, observed.request_id)?
                        .ok_or(StorageError::EntityNotFound)?;
                    let revision = crate::library_store::advance_playback_revision(transaction)?;
                    apply_feed_observation(
                        transaction,
                        &current,
                        &observed,
                        applied_action,
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
        Ok(FeedFetchObservationCommitOutcome {
            replayed: receipt.replayed,
            workflow: self
                .feed_fetch_workflows_snapshot(pod0_application::MAX_ACTIVE_FEED_FETCH_WORKFLOWS)?
                .into_iter()
                .find(|record| record.request_id == input.observation.request_id),
        })
    }
}

fn retry_effect(
    current: &FeedFetchWorkflowRecord,
    action: &FeedFetchLeasedObservationAction,
    revision: StateRevision,
) -> Result<Option<DurableFeedEffectRequest>, StorageError> {
    let FeedFetchLeasedObservationAction::Fail {
        retry_at_ms: Some(not_before),
        retry_deadline_at_ms: Some(deadline),
        ..
    } = action
    else {
        return Ok(None);
    };
    let attempt = current
        .attempt
        .checked_add(1)
        .ok_or(StorageError::CommandConflict)?;
    Ok(Some(DurableFeedEffectRequest {
        request_id: feed_fetch_request_id(&current.feed_key, current.command_id, attempt),
        command_id: current.command_id,
        cancellation_id: current.cancellation_id,
        issued_revision: StateRevision::new(
            revision
                .value
                .checked_add(1)
                .ok_or(StorageError::InvalidActivity)?,
        ),
        not_before: Some(UnixTimestampMilliseconds::new(*not_before)),
        deadline_at: Some(UnixTimestampMilliseconds::new(*deadline)),
        action: DurableFeedEffectAction::FetchFeed {
            podcast_id: current.podcast_id,
            feed_url: current.source_url.clone(),
            entity_tag: current.entity_tag.clone(),
            last_modified: current.last_modified.clone(),
        },
    }))
}

fn apply_feed_observation(
    transaction: &Transaction<'_>,
    current: &FeedFetchWorkflowRecord,
    observation: &pod0_application::DurableFeedHostObservation,
    action: FeedFetchLeasedObservationAction,
    revision: StateRevision,
) -> Result<(), StorageError> {
    match action {
        FeedFetchLeasedObservationAction::Apply {
            parsed,
            entity_tag,
            last_modified,
        } => {
            let mut episodes = parsed.episodes;
            if current.intent == StoredFeedFetchIntent::Metadata {
                episodes.clear();
            }
            crate::library_store_feed_observed::apply_observed_feed(
                transaction,
                current.command_id,
                parsed.podcast,
                episodes,
                current.intent == StoredFeedFetchIntent::Subscribe,
                current.intent == StoredFeedFetchIntent::Refresh,
                entity_tag,
                last_modified,
                observation.observed_at.value,
                revision,
            )?;
            delete_feed_workflow(transaction, current.request_id)?;
        }
        FeedFetchLeasedObservationAction::NotModified {
            entity_tag,
            last_modified,
        } => {
            if matches!(
                current.intent,
                StoredFeedFetchIntent::Refresh | StoredFeedFetchIntent::Metadata
            ) {
                let changed = transaction
                    .execute(
                        "UPDATE pod0_podcasts SET last_refreshed_at_ms=?1,etag=COALESCE(?2,etag),\
                     last_modified=COALESCE(?3,last_modified) WHERE podcast_id=?4",
                        params![
                            observation.observed_at.value,
                            entity_tag,
                            last_modified,
                            current.podcast_id.into_bytes().as_slice()
                        ],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("apply leased not-modified feed", error)
                    })?;
                if changed != 1 {
                    return Err(StorageError::EntityNotFound);
                }
            }
            delete_feed_workflow(transaction, current.request_id)?;
        }
        FeedFetchLeasedObservationAction::Fail {
            failure_code,
            retry_at_ms,
            retry_deadline_at_ms,
        } => {
            if let (Some(not_before), Some(deadline)) = (retry_at_ms, retry_deadline_at_ms) {
                let attempt = current
                    .attempt
                    .checked_add(1)
                    .ok_or(StorageError::CommandConflict)?;
                let request_id =
                    feed_fetch_request_id(&current.feed_key, current.command_id, attempt);
                transaction
                    .execute(
                        "UPDATE pod0_feed_fetch_workflows SET stage='retry_scheduled',attempt=?1,\
                     request_id=?2,issued_revision=?3,deadline_at_ms=?4,not_before_ms=?5,\
                     failure_code=?6,updated_at_ms=?7 WHERE request_id=?8",
                        params![
                            i64::from(attempt),
                            request_id.into_bytes().as_slice(),
                            i64::try_from(revision.value)
                                .map_err(|_| StorageError::InvalidActivity)?,
                            deadline,
                            not_before,
                            failure_code,
                            observation.observed_at.value,
                            current.request_id.into_bytes().as_slice()
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("schedule leased feed retry", error))?;
            } else {
                transaction
                    .execute(
                        "UPDATE pod0_feed_fetch_workflows SET stage='failed',not_before_ms=NULL,\
                     failure_code=?1,updated_at_ms=?2 WHERE request_id=?3",
                        params![
                            failure_code,
                            observation.observed_at.value,
                            current.request_id.into_bytes().as_slice()
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("fail leased feed fetch", error))?;
            }
        }
        FeedFetchLeasedObservationAction::Cancel => {
            delete_feed_workflow(transaction, current.request_id)?;
        }
    }
    Ok(())
}

fn delete_feed_workflow(
    transaction: &Transaction<'_>,
    request_id: HostRequestId,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "DELETE FROM pod0_feed_fetch_workflows WHERE request_id=?1",
            [request_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("complete leased feed workflow", error))?;
    Ok(())
}
