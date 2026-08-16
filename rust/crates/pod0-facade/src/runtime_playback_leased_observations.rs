use pod0_application::{
    ActivityFailureCode, ApplicationCommand, CommandEnvelope, DurableEffectExecution,
    DurablePlaybackEffectAction, EffectOutcome, HostFailureCode, HostObservation,
    HostObservationReceipt, HostObservationRejection, LeasedHostObservationEnvelope,
    PlaybackCommand,
};
use pod0_storage::PlaybackObservationCommitInput;

use crate::runtime_chapter_model_receipts::{persisted, retain};
use crate::runtime_leased_observations::rejected_payload;
use crate::runtime_state::FacadeState;

impl FacadeState {
    pub(super) fn record_leased_playback_observation(
        &mut self,
        leased: LeasedHostObservationEnvelope,
    ) -> (bool, HostObservationReceipt) {
        let request_id = leased.observation.request_id;
        let Some(store) = self.store.clone() else {
            return (false, retain(request_id));
        };
        let request = match store.playback_effect_request(leased.lease.intent_id) {
            Ok(request) => request,
            Err(_) => return (false, retain(request_id)),
        };
        let DurableEffectExecution::Playback { request } = request.execution else {
            return mismatched(request_id);
        };
        let is_stream = matches!(
            request.action,
            DurablePlaybackEffectAction::ObservePlayback { .. }
        );
        let (outcome, terminal) = match &leased.observation.observation {
            HostObservation::PlaybackObserved { .. } => (
                if is_stream {
                    EffectOutcome::Progressed
                } else {
                    EffectOutcome::Succeeded
                },
                !is_stream,
            ),
            HostObservation::Failed { code, .. } => (
                EffectOutcome::Failed {
                    code: activity_failure(*code),
                },
                true,
            ),
            HostObservation::Cancelled => (EffectOutcome::Cancelled, true),
            HostObservation::Unsupported { wire_code } => (
                EffectOutcome::Failed {
                    code: ActivityFailureCode::Unsupported {
                        wire_code: *wire_code,
                    },
                },
                true,
            ),
            _ => return mismatched(request_id),
        };

        if is_stream
            && let HostObservation::PlaybackObserved { value } = &leased.observation.observation
            && !self.playback_observation_is_semantic(value, leased.observation.observed_at.value)
        {
            return match store
                .accept_transient_playback_observation(leased.lease, &leased.observation)
            {
                Ok(()) => {
                    let value = value.clone();
                    let root = observation_root(&leased.observation);
                    let Some(plan) = self.plan_playback_observation(
                        &root,
                        leased.observation.observed_at.value,
                        &value,
                    ) else {
                        return (false, retain(request_id));
                    };
                    debug_assert!(plan.reaction.is_none());
                    self.apply_playback_observation_update(
                        value,
                        leased.observation.observed_at.value,
                        plan,
                    );
                    (
                        false,
                        HostObservationReceipt::AcceptedTransient { request_id },
                    )
                }
                Err(pod0_storage::StorageError::RevisionConflict) => duplicate(request_id),
                Err(_) => (false, retain(request_id)),
            };
        }

        let planned = match &leased.observation.observation {
            HostObservation::PlaybackObserved { value } => {
                let root = observation_root(&leased.observation);
                match self.plan_playback_observation(
                    &root,
                    leased.observation.observed_at.value,
                    value,
                ) {
                    Some(plan) => Some(plan),
                    None => return (false, retain(request_id)),
                }
            }
            _ => None,
        };
        let reaction = planned.as_ref().and_then(|plan| plan.reaction.clone());
        let committed = store.commit_playback_observation(PlaybackObservationCommitInput {
            lease: leased.lease,
            observation: leased.observation.clone(),
            outcome,
            terminal,
            reaction,
            committed_at: self.now(),
        });
        let committed = match committed {
            Ok(committed) => committed,
            Err(pod0_storage::StorageError::RevisionConflict) => return duplicate(request_id),
            Err(_) => return (false, retain(request_id)),
        };
        if committed.replayed {
            return duplicate(request_id);
        }
        if terminal && is_stream {
            self.playback.observation_request_id = None;
        }
        match leased.observation.observation {
            HostObservation::PlaybackObserved { value } => {
                if planned.as_ref().is_some_and(|plan| plan.reaction.is_some())
                    && self.reload_listening().is_err()
                {
                    self.playback.policy_state = pod0_application::PlaybackPolicyState::Failed;
                    return (true, persisted(request_id, terminal));
                }
                self.apply_playback_observation_update(
                    value,
                    leased.observation.observed_at.value,
                    planned.expect("playback observations are planned before commit"),
                );
            }
            HostObservation::Failed { .. }
            | HostObservation::Cancelled
            | HostObservation::Unsupported { .. } => {
                self.playback_host_failed(request.command_id);
            }
            _ => unreachable!("playback observation was validated above"),
        }
        self.advance_revision();
        (true, persisted(request_id, terminal))
    }

    fn playback_observation_is_semantic(
        &self,
        value: &pod0_application::PlaybackLifecycleObservation,
        observed_at_ms: i64,
    ) -> bool {
        let Some(previous) = self.playback.last_observation.as_ref() else {
            return true;
        };
        if previous.episode_id != value.episode_id
            || previous.state != value.state
            || previous.route != value.route
            || previous.interruption != value.interruption
            || previous.ended != value.ended
        {
            return true;
        }
        let Some(episode_id) = value.episode_id else {
            return false;
        };
        let Some(episode) = self
            .listening
            .episodes
            .iter()
            .find(|episode| episode.episode_id == episode_id)
        else {
            return false;
        };
        pod0_domain::should_commit_position(
            episode.listening.resume_position_milliseconds,
            value.position_milliseconds,
            self.playback
                .last_position_commit_at_ms
                .map(pod0_domain::UnixTimestampMilliseconds::new),
            pod0_domain::UnixTimestampMilliseconds::new(observed_at_ms),
            false,
        )
    }
}

pub(super) fn observation_root(
    observation: &pod0_application::HostObservationEnvelope,
) -> CommandEnvelope {
    let mut hash = sha2::Sha256::new();
    use sha2::Digest as _;
    hash.update(b"pod0-playback-observation-reaction-v1\0");
    hash.update(observation.request_id.into_bytes());
    hash.update(observation.sequence_number.to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    CommandEnvelope {
        command_id: pod0_domain::CommandId::from_bytes(
            digest[..16].try_into().expect("fixed digest prefix"),
        ),
        cancellation_id: observation.cancellation_id,
        expected_revision: None,
        command: ApplicationCommand::Playback {
            command: PlaybackCommand::Restore,
        },
    }
}

fn activity_failure(code: HostFailureCode) -> ActivityFailureCode {
    match code {
        HostFailureCode::Offline => ActivityFailureCode::Offline,
        HostFailureCode::TimedOut => ActivityFailureCode::TimedOut,
        HostFailureCode::PermissionDenied => ActivityFailureCode::PermissionDenied,
        HostFailureCode::InvalidResponse => ActivityFailureCode::InvalidResponse,
        HostFailureCode::ResponseTooLarge => ActivityFailureCode::ResponseTooLarge,
        HostFailureCode::MediaUnavailable => ActivityFailureCode::MediaUnavailable,
        HostFailureCode::ProviderUnavailable => ActivityFailureCode::ProviderUnavailable,
        HostFailureCode::Unauthorized => ActivityFailureCode::Unauthorized,
        HostFailureCode::IndexUnavailable | HostFailureCode::PlatformFailure => {
            ActivityFailureCode::PlatformFailure
        }
        HostFailureCode::Unsupported { wire_code } => {
            ActivityFailureCode::Unsupported { wire_code }
        }
    }
}

fn mismatched(request_id: pod0_domain::HostRequestId) -> (bool, HostObservationReceipt) {
    (
        false,
        rejected_payload(request_id, HostObservationRejection::MismatchedPayload),
    )
}

fn duplicate(request_id: pod0_domain::HostRequestId) -> (bool, HostObservationReceipt) {
    (
        false,
        rejected_payload(request_id, HostObservationRejection::Duplicate),
    )
}
