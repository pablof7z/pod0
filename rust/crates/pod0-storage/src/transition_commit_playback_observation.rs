use pod0_application::{
    DurableEffectExecution, DurableExternalEffectRequest, DurablePlaybackEffectAction,
    DurablePlaybackEffectRequest, EffectOutcome, HostObservationEnvelope,
    PersistedEffectLeaseIdentity, PlaybackObservationActivityInput,
    PlaybackObservationReactionActivityInput, PlaybackTransition, plan_playback_observation,
    playback_observation_identity,
};
use pod0_domain::{CommandId, EpisodeId, UnixTimestampMilliseconds};

use super::TransitionCommit;
use crate::{
    EffectOutboxError, LibraryStore, StorageError, TransitionIngress, TransitionIngressKind,
};

#[derive(Clone, Debug)]
pub struct PlaybackObservationCommitInput {
    pub lease: PersistedEffectLeaseIdentity,
    pub observation: HostObservationEnvelope,
    pub outcome: EffectOutcome,
    pub terminal: bool,
    pub reaction: Option<PlaybackObservationReaction>,
    pub committed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackObservationReaction {
    pub command_id: CommandId,
    pub mutation: crate::PlaybackMutation,
    pub episode_id: Option<EpisodeId>,
    pub transition: PlaybackTransition,
    pub effects: Vec<DurablePlaybackEffectRequest>,
}

impl LibraryStore {
    pub fn playback_effect_request(
        &self,
        intent_id: pod0_domain::EffectIntentId,
    ) -> Result<DurableExternalEffectRequest, StorageError> {
        effect_request(self.path(), intent_id)
    }

    pub fn validate_playback_observation_lease(
        &self,
        lease: PersistedEffectLeaseIdentity,
        observation: &HostObservationEnvelope,
    ) -> Result<(), StorageError> {
        self.read(|connection| {
            crate::effect_outbox::validate_playback_observation_lease_in_transaction(
                connection,
                lease,
                observation,
            )
            .map_err(effect_error)
        })
    }

    pub fn accept_transient_playback_observation(
        &self,
        lease: PersistedEffectLeaseIdentity,
        observation: &HostObservationEnvelope,
    ) -> Result<(), StorageError> {
        crate::EffectOutbox::open(self.path())
            .map_err(effect_error)?
            .accept_transient_playback_observation(
                lease,
                observation,
                super::playback_observation_fingerprint::fingerprint(observation),
            )
            .map_err(effect_error)
    }

    pub fn commit_playback_observation(
        &self,
        input: PlaybackObservationCommitInput,
    ) -> Result<crate::CommitReceipt, StorageError> {
        commit(self.path(), input)
    }
}

fn commit(
    path: &std::path::Path,
    input: PlaybackObservationCommitInput,
) -> Result<crate::CommitReceipt, StorageError> {
    let fingerprint = super::playback_observation_fingerprint::fingerprint(&input.observation);
    let identity =
        playback_observation_identity(input.lease.attempt_id, input.observation.sequence_number);
    let request = effect_request(path, input.lease.intent_id)?;
    let DurableEffectExecution::Playback { request } = request.execution else {
        return Err(StorageError::InvalidActivity);
    };
    let episode_id = request.episode_id();
    let observation = input.observation.clone();
    let supersede_streams = input.reaction.as_ref().is_some_and(|reaction| {
        reaction.effects.iter().any(|effect| {
            matches!(
                effect.action,
                DurablePlaybackEffectAction::ObservePlayback { .. }
            )
        })
    });
    let planning_reaction = input.reaction.clone();
    let applying_reaction = input.reaction;
    TransitionCommit::open(path)?.commit_planned_with_transaction_hooks(
        TransitionIngress {
            kind: TransitionIngressKind::HostObservation,
            id: identity.into_bytes(),
            fingerprint,
        },
        input.committed_at,
        |transaction| {
            let superseded = if supersede_streams {
                super::playback_effects::active_observation_effects(transaction)?
            } else {
                Vec::new()
            };
            let reaction = planning_reaction.clone().map(|reaction| {
                PlaybackObservationReactionActivityInput {
                    command_id: reaction.command_id,
                    transition: reaction.transition,
                    checkpoint_position_milliseconds: checkpoint(&reaction.mutation),
                    effects: reaction.effects,
                    superseded_effects: superseded.iter().map(|value| value.target).collect(),
                }
            });
            plan_playback_observation(PlaybackObservationActivityInput {
                identity_attempt_id: identity,
                effect_attempt_id: input.lease.attempt_id,
                request_id: input.observation.request_id,
                command_id: request.command_id,
                episode_id,
                current_revision: playback_revision(transaction)?,
                intent_id: input.lease.intent_id,
                authorizing_activity_id: input.lease.authorizing_activity_id,
                correlation_id: input.lease.correlation_id,
                outcome: input.outcome,
                reaction,
            })
            .map(|plan| {
                plan.map_mutation(|()| PlaybackObservationCommitContext {
                    reaction: applying_reaction,
                    superseded_intents: superseded
                        .into_iter()
                        .map(|value| value.intent_id)
                        .collect(),
                })
            })
            .map_err(|_| StorageError::InvalidActivity)
        },
        |transaction| {
            if input.terminal {
                crate::effect_outbox::stage_playback_observation_in_transaction(
                    transaction,
                    input.lease,
                    &observation,
                    fingerprint,
                    input.outcome,
                )
            } else {
                crate::effect_outbox::record_playback_progress_in_transaction(
                    transaction,
                    input.lease,
                    &observation,
                    fingerprint,
                )
            }
            .map_err(effect_error)
        },
        |transaction, expected, context| {
            if playback_revision(transaction)? != expected {
                return Err(StorageError::RevisionConflict);
            }
            super::playback_effects::supersede_effects(transaction, &context.superseded_intents)?;
            let Some(reaction) = context.reaction else {
                return Ok(expected);
            };
            crate::library_store_playback_apply::apply_mutation(
                transaction,
                reaction.mutation,
                input.committed_at.value,
            )?;
            crate::library_store::advance_playback_revision(transaction)
        },
        |transaction| {
            if input.terminal {
                crate::effect_outbox::complete_host_observation_in_transaction(
                    transaction,
                    input.lease,
                )
                .map_err(effect_error)?;
            }
            Ok(())
        },
    )
}

struct PlaybackObservationCommitContext {
    reaction: Option<PlaybackObservationReaction>,
    superseded_intents: Vec<[u8; 16]>,
}

fn checkpoint(mutation: &crate::PlaybackMutation) -> Option<u64> {
    match mutation {
        crate::PlaybackMutation::Checkpoint {
            position_milliseconds,
            ..
        }
        | crate::PlaybackMutation::CheckpointAndAdvanceQueue {
            position_milliseconds,
            ..
        }
        | crate::PlaybackMutation::CheckpointAndFinishActive {
            position_milliseconds,
            ..
        } => Some(*position_milliseconds),
        _ => None,
    }
}

fn effect_request(
    path: &std::path::Path,
    intent_id: pod0_domain::EffectIntentId,
) -> Result<DurableExternalEffectRequest, StorageError> {
    LibraryStore::open_authoritative(path)?.read(|connection| {
        let payload: String = connection
            .query_row(
                "SELECT request_json FROM pod0_effect_intents \
                 WHERE intent_id=?1 AND effect_kind_code=2",
                [intent_id.into_bytes().as_slice()],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::sqlite("read playback effect request", error))?;
        serde_json::from_str(&payload).map_err(|_| StorageError::InvalidActivity)
    })
}

fn playback_revision(
    connection: &rusqlite::Connection,
) -> Result<pod0_domain::StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read playback observation revision", error))?;
    u64::try_from(value)
        .map(pod0_domain::StateRevision::new)
        .map_err(|_| StorageError::InvalidActivity)
}

fn effect_error(error: EffectOutboxError) -> StorageError {
    match error {
        EffectOutboxError::StaleLease => StorageError::RevisionConflict,
        _ => StorageError::InvalidActivity,
    }
}
