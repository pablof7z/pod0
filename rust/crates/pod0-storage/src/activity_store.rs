use std::path::{Path, PathBuf};

use pod0_application::{
    ActivityFact, ActivityFactDraft, CommittedActivityFact, NonEmptyActivityFacts,
};
use pod0_domain::UnixTimestampMilliseconds;
use rusqlite::{Connection, Transaction, params};

use crate::activity_store_codec::{
    actor_code, fact_code, origin_code, subject, verify_stored_draft,
};
use crate::activity_store_model::{ActivityPage, MAX_ACTIVITY_PAGE_ITEMS};
use crate::migration_db::{
    open_connection, user_version, validate_current_database_identity, validate_open_database,
};
use crate::transition_commit::JournalAppendAuthority;
use crate::{CURRENT_SCHEMA_VERSION, StorageError};

#[derive(Clone, Debug)]
pub struct ActivityStore {
    pub(super) path: PathBuf,
}

impl ActivityStore {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = open_current(path, true)?;
        validate_open_database(&connection, CURRENT_SCHEMA_VERSION)?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    pub fn page_for_episode(
        &self,
        episode_id: pod0_domain::EpisodeId,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> Result<ActivityPage, StorageError> {
        self.page(
            "sequence IN (WITH RECURSIVE linked(activity_id,sequence) AS (\
             SELECT activity_id,sequence FROM pod0_activity_facts WHERE episode_id=?1 \
             UNION SELECT parent.activity_id,parent.sequence FROM pod0_activity_facts parent \
             JOIN pod0_activity_facts child ON child.caused_by_activity_id=parent.activity_id \
             JOIN linked ON child.activity_id=linked.activity_id) SELECT sequence FROM linked) \
             AND sequence>?2",
            episode_id.into_bytes().as_slice(),
            after_sequence,
            requested_count,
        )
    }

    pub fn page_for_correlation(
        &self,
        correlation_id: pod0_domain::ActivityCorrelationId,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> Result<ActivityPage, StorageError> {
        self.page(
            "correlation_id=?1 AND sequence>?2",
            correlation_id.into_bytes().as_slice(),
            after_sequence,
            requested_count,
        )
    }

    pub fn page_for_operation(
        &self,
        command_id: pod0_domain::CommandId,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> Result<ActivityPage, StorageError> {
        self.page(
            "correlation_id IN (SELECT correlation_id FROM pod0_activity_facts \
             WHERE command_id=?1) AND sequence>?2",
            command_id.into_bytes().as_slice(),
            after_sequence,
            requested_count,
        )
    }

    pub fn page_for_support(
        &self,
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> Result<ActivityPage, StorageError> {
        self.page(
            "length(?1)>=0 AND sequence>?2",
            &[],
            after_sequence,
            requested_count,
        )
    }

    fn page(
        &self,
        predicate: &str,
        identity: &[u8],
        after_sequence: Option<u64>,
        requested_count: u16,
    ) -> Result<ActivityPage, StorageError> {
        let connection = open_current(&self.path, true)?;
        let after = i64::try_from(after_sequence.unwrap_or(0))
            .map_err(|_| StorageError::CommandConflict)?;
        let page_size = requested_count.clamp(1, MAX_ACTIVITY_PAGE_ITEMS);
        let limit = i64::from(page_size) + 1;
        let sql = format!(
            "SELECT sequence,committed_at_ms,payload_json,activity_id,transaction_id,correlation_id,\
             caused_by_activity_id,command_id,host_request_id,actor_code,origin_code,subject_code,\
             subject_id,episode_id,fact_code FROM pod0_activity_facts \
             WHERE {predicate} ORDER BY sequence ASC LIMIT ?3"
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| StorageError::sqlite("prepare activity page", error))?;
        let rows = statement
            .query_map(params![identity, after, limit], decode_committed)
            .map_err(|error| StorageError::sqlite("query activity page", error))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|error| StorageError::sqlite("decode activity page", error))?);
        }
        let has_more = items.len() > usize::from(page_size);
        if has_more {
            items.pop();
        }
        let next_after_sequence = has_more.then(|| {
            items
                .last()
                .expect("a non-empty bounded activity page")
                .sequence
        });
        Ok(ActivityPage {
            items,
            next_after_sequence,
        })
    }
}

pub(crate) fn append_activity_facts(
    _authority: &JournalAppendAuthority,
    transaction: &Transaction<'_>,
    facts: &NonEmptyActivityFacts,
    committed_at: UnixTimestampMilliseconds,
) -> Result<Vec<CommittedActivityFact>, StorageError> {
    let expected_transaction = facts.get(0).expect("non-empty facts").transaction_id;
    let mut committed = Vec::with_capacity(facts.len());
    for draft in facts.iter() {
        if draft.transaction_id != expected_transaction {
            return Err(StorageError::CommandConflict);
        }
        let payload = serde_json::to_string(draft).map_err(|_| StorageError::InvalidActivity)?;
        let (subject_code, subject_id) = subject(draft.subject);
        let authorized_effect_intent_id = match draft.fact {
            ActivityFact::EffectAuthorized { intent_id, .. } => Some(intent_id.into_bytes()),
            _ => None,
        };
        let authorized_internal_command_id = match draft.fact {
            ActivityFact::InternalCommandAuthorized {
                internal_command_id,
                ..
            } => Some(internal_command_id.into_bytes()),
            _ => None,
        };
        transaction
            .execute(
                "INSERT INTO pod0_activity_facts(activity_id,transaction_id,correlation_id,\
                 caused_by_activity_id,command_id,host_request_id,authorized_effect_intent_id,\
                 authorized_internal_command_id,actor_code,origin_code,subject_code,subject_id,\
                 episode_id,fact_code,payload_schema_version,payload_json,committed_at_ms) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,1,?15,?16)",
                params![
                    draft.activity_id.into_bytes().as_slice(),
                    draft.transaction_id.into_bytes().as_slice(),
                    draft.correlation_id.into_bytes().as_slice(),
                    draft.caused_by_activity_id.map(|value| value.into_bytes()),
                    draft.command_id.map(|value| value.into_bytes()),
                    draft.host_request_id.map(|value| value.into_bytes()),
                    authorized_effect_intent_id,
                    authorized_internal_command_id,
                    actor_code(draft.actor),
                    origin_code(draft.origin),
                    subject_code,
                    subject_id,
                    draft.episode_id.map(|value| value.into_bytes()),
                    fact_code(draft.fact),
                    payload,
                    committed_at.value,
                ],
            )
            .map_err(|error| StorageError::sqlite("append activity fact", error))?;
        let sequence = u64::try_from(transaction.last_insert_rowid()).map_err(|_| {
            StorageError::CorruptSchema {
                detail: "activity sequence is invalid",
            }
        })?;
        committed.push(CommittedActivityFact {
            sequence,
            committed_at,
            draft: *draft,
        });
    }
    Ok(committed)
}

pub(super) fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_current_database_identity(&connection, version)?;
    Ok(connection)
}

pub(super) fn decode_committed(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommittedActivityFact> {
    let sequence: i64 = row.get(0)?;
    let committed_at: i64 = row.get(1)?;
    let payload: String = row.get(2)?;
    let draft: ActivityFactDraft = serde_json::from_str(&payload).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    verify_stored_draft(row, &draft)?;
    let sequence = u64::try_from(sequence).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    Ok(CommittedActivityFact {
        sequence,
        committed_at: UnixTimestampMilliseconds::new(committed_at),
        draft,
    })
}
