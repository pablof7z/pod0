use pod0_application::{
    DurableFeedEffectAction, DurableFeedEffectRequest, FeedFetchActivityInput,
    FeedFetchActivityMutation, plan_feed_fetch,
};

impl LibraryStore {
    pub(crate) fn commit_feed_fetch_admission(
        &self,
        input: FeedFetchEnsureInput,
    ) -> Result<FeedFetchEnsureOutcome, StorageError> {
        let command_id = input.command_id;
        let fingerprint = input.command_fingerprint.clone();
        let feed_key = input.feed_key.clone();
        let receipt = TransitionCommit::open(self.path())?.commit_planned_with(
            TransitionIngress {
                kind: TransitionIngressKind::ApplicationCommand,
                id: command_id.into_bytes(),
                fingerprint: fingerprint_digest(&fingerprint)?,
            },
            UnixTimestampMilliseconds::new(input.now_ms),
            |transaction| {
                let current = current_library_revision(transaction)?;
                let existing = podcast_id_for_feed_key(transaction, &input.feed_key)?;
                let podcast_id = existing.unwrap_or(input.podcast_id);
                let active = workflow_for_feed(transaction, &input.feed_key)?
                    .filter(|record| record.stage != StoredFeedFetchStage::Failed);
                let semantic_change = active
                    .as_ref()
                    .is_none_or(|record| input.intent > record.intent);
                let should_authorize = active.is_none();
                let request_id = feed_fetch_request_id(&input.feed_key, input.command_id, 1);
                let committed_revision = StateRevision::new(
                    current
                        .value
                        .checked_add(1)
                        .ok_or(StorageError::InvalidActivity)?,
                );
                let effect = should_authorize.then(|| DurableFeedEffectRequest {
                    request_id,
                    command_id: input.command_id,
                    cancellation_id: input.cancellation_id,
                    issued_revision: committed_revision,
                    not_before: None,
                    deadline_at: Some(UnixTimestampMilliseconds::new(input.deadline_at_ms)),
                    action: DurableFeedEffectAction::FetchFeed {
                        podcast_id,
                        feed_url: input.source_url.clone(),
                        entity_tag: input.entity_tag.clone(),
                        last_modified: input.last_modified.clone(),
                    },
                });
                plan_feed_fetch(FeedFetchActivityInput {
                    command_id: input.command_id,
                    podcast_id,
                    current_revision: current,
                    legacy_command_revision: None,
                    semantic_change,
                    effect,
                })
                .map(|plan| {
                    plan.map_mutation(|mutation| match mutation {
                        FeedFetchActivityMutation::Apply => FeedFetchStorageMutation::Apply {
                            podcast_id,
                            existing_parent: existing.is_some(),
                            replace_workflow: should_authorize,
                        },
                        FeedFetchActivityMutation::RecordNoChange => {
                            FeedFetchStorageMutation::RecordNoChange
                        }
                        FeedFetchActivityMutation::Duplicate { committed_revision } => {
                            FeedFetchStorageMutation::Duplicate { committed_revision }
                        }
                    })
                })
                .map_err(|_| StorageError::InvalidActivity)
            },
            |transaction, expected, mutation| match mutation {
                FeedFetchStorageMutation::Apply {
                    podcast_id,
                    existing_parent,
                    replace_workflow,
                } => {
                    require_revision(transaction, expected)?;
                    if !existing_parent {
                        upsert_podcast(transaction, &placeholder_podcast(&input, podcast_id))?;
                    }
                    if input.intent == StoredFeedFetchIntent::Subscribe {
                        insert_subscription(transaction, podcast_id, input.now_ms)?;
                    }
                    if replace_workflow {
                        write_feed_fetch_workflow(transaction, &input, podcast_id, expected)?;
                    } else {
                        transaction
                            .execute(
                                "UPDATE pod0_feed_fetch_workflows SET intent=?1,updated_at_ms=?2 \
                                 WHERE feed_key_v1=?3",
                                params![input.intent.wire(), input.now_ms, input.feed_key],
                            )
                            .map_err(|error| {
                                StorageError::sqlite("coalesce feed fetch intent", error)
                            })?;
                    }
                    finish_command(transaction, command_id, &fingerprint, input.now_ms)
                }
                FeedFetchStorageMutation::RecordNoChange => {
                    require_revision(transaction, expected)?;
                    record_no_change(
                        transaction,
                        command_id,
                        &fingerprint,
                        expected,
                        input.now_ms,
                    )?;
                    Ok(expected)
                }
                FeedFetchStorageMutation::Duplicate { committed_revision } => {
                    Ok(committed_revision)
                }
            },
        )?;
        let record = self
            .feed_fetch_workflows_snapshot(pod0_application::MAX_ACTIVE_FEED_FETCH_WORKFLOWS)?
            .into_iter()
            .find(|record| record.feed_key == feed_key);
        let podcast_id = record
            .as_ref()
            .map(|record| record.podcast_id)
            .or_else(|| {
                self.snapshot()
                    .ok()?
                    .podcasts
                    .into_iter()
                    .find_map(|podcast| {
                        podcast
                            .feed_identity
                            .filter(|identity| identity.comparison_key == feed_key)
                            .map(|_| podcast.podcast_id)
                    })
            })
            .ok_or(StorageError::EntityNotFound)?;
        let _ = receipt;
        Ok(FeedFetchEnsureOutcome { podcast_id, record })
    }
}

enum FeedFetchStorageMutation {
    Apply {
        podcast_id: PodcastId,
        existing_parent: bool,
        replace_workflow: bool,
    },
    RecordNoChange,
    Duplicate {
        committed_revision: StateRevision,
    },
}

fn write_feed_fetch_workflow(
    transaction: &Transaction<'_>,
    input: &FeedFetchEnsureInput,
    podcast_id: PodcastId,
    expected: StateRevision,
) -> Result<(), StorageError> {
    let request_id = feed_fetch_request_id(&input.feed_key, input.command_id, 1);
    let issued_revision = expected
        .value
        .checked_add(1)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(StorageError::InvalidActivity)?;
    transaction
        .execute(
            "INSERT INTO pod0_feed_fetch_workflows(feed_key_v1,source_url,podcast_id,intent,stage,\
         attempt,request_id,command_id,command_fingerprint,cancellation_id,issued_revision,\
         deadline_at_ms,not_before_ms,entity_tag,last_modified,failure_code,created_at_ms,\
         updated_at_ms) VALUES(?1,?2,?3,?4,'requested',1,?5,?6,?7,?8,?9,?10,NULL,?11,?12,\
         NULL,?13,?13) ON CONFLICT(feed_key_v1) DO UPDATE SET source_url=excluded.source_url,\
         podcast_id=excluded.podcast_id,intent=excluded.intent,stage='requested',attempt=1,\
         request_id=excluded.request_id,command_id=excluded.command_id,\
         command_fingerprint=excluded.command_fingerprint,cancellation_id=excluded.cancellation_id,\
         issued_revision=excluded.issued_revision,deadline_at_ms=excluded.deadline_at_ms,\
         not_before_ms=NULL,entity_tag=excluded.entity_tag,last_modified=excluded.last_modified,\
         failure_code=NULL,updated_at_ms=excluded.updated_at_ms",
            params![
                input.feed_key,
                input.source_url,
                podcast_id.into_bytes().as_slice(),
                input.intent.wire(),
                request_id.into_bytes().as_slice(),
                input.command_id.into_bytes().as_slice(),
                input.command_fingerprint,
                input.cancellation_id.into_bytes().as_slice(),
                issued_revision,
                input.deadline_at_ms,
                input.entity_tag,
                input.last_modified,
                input.now_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("insert feed fetch workflow", error))?;
    Ok(())
}
