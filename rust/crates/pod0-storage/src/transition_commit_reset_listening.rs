use pod0_application::{
    CancellationEffectTarget, DurableEffectExecution, DurableExternalEffectRequest,
    DurablePlaybackEffectRequest, ListeningResetActivityInput, ListeningResetMutation,
    plan_listening_reset,
};
use pod0_domain::{CommandId, StateRevision, UnixTimestampMilliseconds};

use super::TransitionCommit;
use super::application_support::{fingerprint as fingerprint_digest, legacy_library_receipt};
use crate::library_store_clip_support::set_clip_revision;
use crate::library_store_note_support::finish_note_command;
use crate::{LibraryStore, StorageError, TransitionIngress, TransitionIngressKind};

impl LibraryStore {
    pub fn reset_listening_data_with_effects(
        &self,
        command_id: CommandId,
        fingerprint: &str,
        effects: Vec<DurablePlaybackEffectRequest>,
        observed_at_ms: i64,
    ) -> Result<StateRevision, StorageError> {
        commit(
            self.path(),
            command_id,
            fingerprint,
            effects,
            observed_at_ms,
        )
    }
}

fn commit(
    path: &std::path::Path,
    command_id: CommandId,
    fingerprint: &str,
    effects: Vec<DurablePlaybackEffectRequest>,
    observed_at_ms: i64,
) -> Result<StateRevision, StorageError> {
    let receipt = TransitionCommit::open(path)?.commit_planned_with(
        TransitionIngress {
            kind: TransitionIngressKind::ApplicationCommand,
            id: command_id.into_bytes(),
            fingerprint: fingerprint_digest(fingerprint)?,
        },
        UnixTimestampMilliseconds::new(observed_at_ms),
        |transaction| {
            let current = playback_revision(transaction)?;
            let legacy = legacy_library_receipt(
                transaction,
                command_id,
                fingerprint,
                "read reset listening command",
            )?;
            let active = active_playback_effects(transaction)?;
            let superseded_effects = active.iter().map(|value| value.target).collect();
            let superseded_intents = active.into_iter().map(|value| value.intent_id).collect();
            plan_listening_reset(ListeningResetActivityInput {
                command_id,
                current_revision: current,
                legacy_command_revision: legacy,
                effects,
                superseded_effects,
            })
            .map(|plan| {
                plan.map_mutation(|mutation| match mutation {
                    ListeningResetMutation::Reset => ResetContext::Reset { superseded_intents },
                    ListeningResetMutation::Duplicate { committed_revision } => {
                        ResetContext::Duplicate { committed_revision }
                    }
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction, expected, context| match context {
            ResetContext::Duplicate { committed_revision } => Ok(committed_revision),
            ResetContext::Reset { superseded_intents } => {
                require_revision(transaction, expected)?;
                supersede_effects(transaction, &superseded_intents)?;
                apply_reset(transaction)?;
                let revision =
                    finish_note_command(transaction, command_id, fingerprint, observed_at_ms)?;
                set_clip_revision(transaction, revision)?;
                Ok(revision)
            }
        },
    )?;
    Ok(receipt.committed_revision)
}

enum ResetContext {
    Reset { superseded_intents: Vec<[u8; 16]> },
    Duplicate { committed_revision: StateRevision },
}

#[derive(Clone, Copy)]
struct ActiveEffect {
    intent_id: [u8; 16],
    target: CancellationEffectTarget,
}

fn active_playback_effects(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<Vec<ActiveEffect>, StorageError> {
    let mut statement = transaction
        .prepare(
            "SELECT intent_id,request_json FROM pod0_effect_intents \
             WHERE effect_kind_code=2 AND state_code IN(1,2) ORDER BY committed_at_ms,intent_id",
        )
        .map_err(|error| StorageError::sqlite("read reset playback effects", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| StorageError::sqlite("query reset playback effects", error))?;
    let mut effects = Vec::new();
    for row in rows {
        let (intent_id, payload) =
            row.map_err(|error| StorageError::sqlite("decode reset playback effect", error))?;
        let request: DurableExternalEffectRequest =
            serde_json::from_str(&payload).map_err(|_| StorageError::InvalidActivity)?;
        let DurableEffectExecution::Playback { request: playback } = request.execution else {
            return Err(StorageError::InvalidActivity);
        };
        effects.push(ActiveEffect {
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

fn supersede_effects(
    transaction: &rusqlite::Transaction<'_>,
    intents: &[[u8; 16]],
) -> Result<(), StorageError> {
    for intent in intents {
        transaction
            .execute(
                "UPDATE pod0_effect_attempts SET state_code=4 WHERE intent_id=?1 AND state_code IN(1,2)",
                [intent.as_slice()],
            )
            .map_err(|error| StorageError::sqlite("supersede reset playback attempt", error))?;
        let changed = transaction
            .execute(
                "UPDATE pod0_effect_intents SET state_code=4 WHERE intent_id=?1 AND state_code IN(1,2)",
                [intent.as_slice()],
            )
            .map_err(|error| StorageError::sqlite("supersede reset playback intent", error))?;
        if changed != 1 {
            return Err(StorageError::RevisionConflict);
        }
    }
    Ok(())
}

include!("transition_commit_reset_listening_apply.rs");
