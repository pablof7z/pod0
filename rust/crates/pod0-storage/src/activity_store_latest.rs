use rusqlite::{Connection, params};

use crate::activity_store::{ActivityStore, decode_committed, open_current};
use crate::{LatestActivityPage, MAX_ACTIVITY_PAGE_ITEMS, StorageError};

impl ActivityStore {
    pub fn latest_page_for_episode(
        &self,
        episode_id: pod0_domain::EpisodeId,
        snapshot_through_sequence: Option<u64>,
        before_sequence: Option<u64>,
        requested_count: u16,
    ) -> Result<LatestActivityPage, StorageError> {
        if snapshot_through_sequence.is_none() != before_sequence.is_none() {
            return Err(StorageError::InvalidActivity);
        }
        let mut connection = open_current(&self.path, true)?;
        let transaction = connection
            .transaction()
            .map_err(|error| StorageError::sqlite("open activity snapshot", error))?;
        let identity = episode_id.into_bytes();
        let current_max = linked_max_sequence(&transaction, &identity)?;
        let snapshot = snapshot_through_sequence.or(current_max);
        if snapshot_through_sequence
            .is_some_and(|value| value == 0 || current_max.is_none_or(|maximum| value > maximum))
        {
            return Err(StorageError::InvalidActivity);
        }
        let before = before_sequence_value(snapshot, before_sequence)?;
        let Some((snapshot, before)) = snapshot.zip(before) else {
            return Ok(LatestActivityPage {
                items: Vec::new(),
                snapshot_through_sequence: None,
                next_before_sequence: None,
            });
        };
        let page_size = requested_count.clamp(1, MAX_ACTIVITY_PAGE_ITEMS);
        let mut statement = transaction
            .prepare(LATEST_PAGE_SQL)
            .map_err(|error| StorageError::sqlite("prepare latest activity page", error))?;
        let rows = statement
            .query_map(
                params![
                    identity.as_slice(),
                    integer(snapshot)?,
                    integer(before)?,
                    i64::from(page_size) + 1
                ],
                decode_committed,
            )
            .map_err(|error| StorageError::sqlite("query latest activity page", error))?;
        let mut items = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::sqlite("decode latest activity page", error))?;
        let has_more = items.len() > usize::from(page_size);
        if has_more {
            items.pop();
        }
        let next_before_sequence =
            has_more.then(|| items.last().expect("non-empty latest page").sequence);
        drop(statement);
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("close activity snapshot", error))?;
        Ok(LatestActivityPage {
            items,
            snapshot_through_sequence: Some(snapshot),
            next_before_sequence,
        })
    }
}

fn before_sequence_value(
    snapshot: Option<u64>,
    before: Option<u64>,
) -> Result<Option<u64>, StorageError> {
    match (snapshot, before) {
        (None, None) => Ok(None),
        (Some(snapshot), None) => snapshot
            .checked_add(1)
            .map(Some)
            .ok_or(StorageError::InvalidActivity),
        (Some(snapshot), Some(before)) if before > 0 && before <= snapshot => Ok(Some(before)),
        _ => Err(StorageError::InvalidActivity),
    }
}

fn linked_max_sequence(
    connection: &Connection,
    episode_id: &[u8],
) -> Result<Option<u64>, StorageError> {
    let maximum: Option<i64> = connection
        .query_row(LINKED_MAX_SQL, [episode_id], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("read activity snapshot maximum", error))?;
    maximum.map(integer_from).transpose()
}

fn integer(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidActivity)
}

fn integer_from(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::InvalidActivity)
}

pub(crate) const LINKED_MAX_SQL: &str = "WITH RECURSIVE linked(activity_id,sequence) AS (\
     SELECT activity_id,sequence FROM pod0_activity_facts WHERE episode_id=?1 \
     UNION SELECT parent.activity_id,parent.sequence FROM pod0_activity_facts parent \
     JOIN pod0_activity_facts child ON child.caused_by_activity_id=parent.activity_id \
     JOIN linked ON child.activity_id=linked.activity_id) SELECT MAX(sequence) FROM linked";

pub(crate) const LATEST_PAGE_SQL: &str = "WITH RECURSIVE linked(activity_id,sequence) AS (\
     SELECT activity_id,sequence FROM pod0_activity_facts WHERE episode_id=?1 AND sequence<=?2 \
     UNION SELECT parent.activity_id,parent.sequence FROM pod0_activity_facts parent \
     JOIN pod0_activity_facts child ON child.caused_by_activity_id=parent.activity_id \
     JOIN linked ON child.activity_id=linked.activity_id WHERE parent.sequence<=?2) \
     SELECT fact.sequence,fact.committed_at_ms,fact.payload_json,fact.activity_id,\
     fact.transaction_id,fact.correlation_id,fact.caused_by_activity_id,fact.command_id,\
     fact.host_request_id,fact.actor_code,fact.origin_code,fact.subject_code,fact.subject_id,\
     fact.episode_id,fact.fact_code \
     FROM pod0_activity_facts fact JOIN linked ON linked.sequence=fact.sequence \
     WHERE fact.sequence<?3 ORDER BY fact.sequence DESC LIMIT ?4";
