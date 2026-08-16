use pod0_application::{
    CancellationEffectTarget, DurableEffectExecution, DurableExternalEffectRequest,
    DurablePlaybackEffectAction,
};

use crate::StorageError;

#[derive(Clone, Copy)]
pub(super) struct SupersededPlaybackEffect {
    pub(super) intent_id: [u8; 16],
    pub(super) target: CancellationEffectTarget,
}

pub(super) fn active_observation_effects(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<Vec<SupersededPlaybackEffect>, StorageError> {
    let mut statement = transaction
        .prepare(
            "SELECT intent_id,request_json FROM pod0_effect_intents \
             WHERE effect_kind_code=2 AND state_code IN(1,2) ORDER BY committed_at_ms,intent_id",
        )
        .map_err(|error| StorageError::sqlite("read active playback observations", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| StorageError::sqlite("query active playback observations", error))?;
    let mut effects = Vec::new();
    for row in rows {
        let (intent_id, payload) =
            row.map_err(|error| StorageError::sqlite("decode active playback observation", error))?;
        let request: DurableExternalEffectRequest =
            serde_json::from_str(&payload).map_err(|_| StorageError::InvalidActivity)?;
        let DurableEffectExecution::Playback { request: playback } = request.execution else {
            return Err(StorageError::InvalidActivity);
        };
        if !matches!(
            playback.action,
            DurablePlaybackEffectAction::ObservePlayback { .. }
        ) {
            continue;
        }
        effects.push(SupersededPlaybackEffect {
            intent_id: intent_id
                .try_into()
                .map_err(|_| StorageError::InvalidActivity)?,
            target: CancellationEffectTarget {
                subject: request.subject,
                episode_id: request.episode_id,
                host_request_id: playback.request_id,
                cancellation_id: playback.cancellation_id,
            },
        });
    }
    Ok(effects)
}

pub(super) fn supersede_effects(
    transaction: &rusqlite::Transaction<'_>,
    intent_ids: &[[u8; 16]],
) -> Result<(), StorageError> {
    for intent_id in intent_ids {
        transaction
            .execute(
                "UPDATE pod0_effect_attempts SET state_code=4 \
                 WHERE intent_id=?1 AND state_code IN(1,2)",
                [intent_id.as_slice()],
            )
            .map_err(|error| StorageError::sqlite("supersede playback stream attempt", error))?;
        let changed = transaction
            .execute(
                "UPDATE pod0_effect_intents SET state_code=4 \
                 WHERE intent_id=?1 AND state_code IN(1,2)",
                [intent_id.as_slice()],
            )
            .map_err(|error| StorageError::sqlite("supersede playback stream intent", error))?;
        if changed != 1 {
            return Err(StorageError::RevisionConflict);
        }
    }
    Ok(())
}
