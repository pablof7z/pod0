use pod0_application::ActivitySubject;
use pod0_domain::ScheduledTaskId;

use crate::StorageError;

#[derive(Clone, Default)]
pub(super) struct AffectedTaskWork {
    pub(super) occurrences: Vec<AffectedOccurrence>,
    pub(super) effects: Vec<ActiveScheduledEffect>,
}

#[derive(Clone, Copy)]
pub(super) struct AffectedOccurrence {
    pub(super) occurrence_id: pod0_domain::ScheduledOccurrenceId,
    pub(super) has_attempt: bool,
}

#[derive(Clone, Copy)]
pub(super) struct ActiveScheduledEffect {
    pub(super) intent_id: [u8; 16],
    pub(super) subject: ActivitySubject,
}

pub(super) fn affected_task_work(
    transaction: &rusqlite::Transaction<'_>,
    task_id: ScheduledTaskId,
) -> Result<AffectedTaskWork, StorageError> {
    let mut occurrences = transaction
        .prepare(
            "SELECT occurrence_id,attempt_id FROM pod0_scheduled_occurrences WHERE task_id=?1 \
             AND stage IN('pending','requested','host_accepted','retry_scheduled','blocked') \
             ORDER BY occurrence_id",
        )
        .map_err(|error| StorageError::sqlite("read removable scheduled runs", error))?;
    let rows = occurrences
        .query_map([task_id.into_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
        })
        .map_err(|error| StorageError::sqlite("query removable scheduled runs", error))?;
    let mut affected = Vec::new();
    for row in rows {
        let (occurrence, attempt) =
            row.map_err(|error| StorageError::sqlite("decode removable scheduled run", error))?;
        affected.push(AffectedOccurrence {
            occurrence_id: crate::scheduled_agent_store_codec::occurrence_id(&occurrence)?,
            has_attempt: attempt.is_some(),
        });
    }
    Ok(AffectedTaskWork {
        occurrences: affected,
        effects: active_task_effects(transaction, task_id)?,
    })
}

fn active_task_effects(
    transaction: &rusqlite::Transaction<'_>,
    task_id: ScheduledTaskId,
) -> Result<Vec<ActiveScheduledEffect>, StorageError> {
    let mut statement = transaction.prepare(
        "SELECT i.intent_id,i.subject_id FROM pod0_effect_intents i JOIN pod0_scheduled_occurrences o \
         ON o.occurrence_id=i.subject_id WHERE i.effect_kind_code=11 AND i.state_code IN(1,2) \
         AND o.task_id=?1 ORDER BY i.intent_id",
    ).map_err(|error| StorageError::sqlite("read active scheduled task effects", error))?;
    let rows = statement
        .query_map([task_id.into_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|error| StorageError::sqlite("query active scheduled task effects", error))?;
    let mut effects = Vec::new();
    for row in rows {
        let (intent, occurrence) = row
            .map_err(|error| StorageError::sqlite("decode active scheduled task effect", error))?;
        effects.push(ActiveScheduledEffect {
            intent_id: intent
                .try_into()
                .map_err(|_| StorageError::InvalidActivity)?,
            subject: ActivitySubject::ScheduledOccurrence {
                occurrence_id: crate::scheduled_agent_store_codec::occurrence_id(&occurrence)?,
            },
        });
    }
    Ok(effects)
}

pub(super) fn active_occurrence_effects(
    transaction: &rusqlite::Transaction<'_>,
    occurrence_id: pod0_domain::ScheduledOccurrenceId,
) -> Result<Vec<ActiveScheduledEffect>, StorageError> {
    let mut statement = transaction
        .prepare(
            "SELECT intent_id FROM pod0_effect_intents WHERE effect_kind_code=11 \
         AND state_code IN(1,2) AND subject_id=?1 ORDER BY intent_id",
        )
        .map_err(|error| StorageError::sqlite("read active scheduled occurrence effects", error))?;
    let rows = statement
        .query_map([occurrence_id.into_bytes().as_slice()], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .map_err(|error| {
            StorageError::sqlite("query active scheduled occurrence effects", error)
        })?;
    let subject = ActivitySubject::ScheduledOccurrence { occurrence_id };
    rows.map(|row| {
        let value = row.map_err(|error| {
            StorageError::sqlite("decode active scheduled occurrence effect", error)
        })?;
        Ok(ActiveScheduledEffect {
            intent_id: value
                .try_into()
                .map_err(|_| StorageError::InvalidActivity)?,
            subject,
        })
    })
    .collect()
}

pub(super) fn supersede_effects(
    transaction: &rusqlite::Transaction<'_>,
    effects: &[ActiveScheduledEffect],
) -> Result<(), StorageError> {
    for effect in effects {
        transaction.execute(
            "UPDATE pod0_effect_attempts SET state_code=4 WHERE state_code IN(1,2) AND intent_id=?1",
            [effect.intent_id.as_slice()],
        ).map_err(|error| StorageError::sqlite("supersede scheduled effect attempts", error))?;
        let changed = transaction.execute(
            "UPDATE pod0_effect_intents SET state_code=4 WHERE state_code IN(1,2) AND intent_id=?1",
            [effect.intent_id.as_slice()],
        ).map_err(|error| StorageError::sqlite("supersede scheduled effect intent", error))?;
        if changed != 1 {
            return Err(StorageError::RevisionConflict);
        }
    }
    Ok(())
}
