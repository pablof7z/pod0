use pod0_domain::{
    ActivityCorrelationId, ActivityId, CancellationId, CommandId, EffectAttemptId, EffectIntentId,
    EpisodeId, HostRequestId, StateRevision, UnixTimestampMilliseconds,
};

use crate::{
    ActivitySubject, CoreWakeReason, DurableEffectExecution, DurableLifecycleEffectRequest,
    EffectOutcome, LifecycleWakeAdmissionInput, LifecycleWakeObservationInput, RequestDisposition,
    plan_lifecycle_wake_admission, plan_lifecycle_wake_observation,
};

fn request(attempt: u8) -> DurableLifecycleEffectRequest {
    DurableLifecycleEffectRequest {
        request_id: HostRequestId::from_parts(1, u64::from(attempt)),
        command_id: CommandId::from_parts(2, 1),
        cancellation_id: CancellationId::from_parts(3, 1),
        issued_revision: StateRevision::new(4),
        wake_at: UnixTimestampMilliseconds::new(10_000),
        reason: CoreWakeReason::TranscriptRetry {
            episode_id: EpisodeId::from_parts(5, 1),
            attempt_id: pod0_domain::TranscriptAttemptId::from_parts(6, 1),
            submission_fence_id: pod0_domain::TranscriptSubmissionFenceId::from_parts(7, 1),
        },
        attempt,
    }
}

#[test]
fn wake_admission_authorizes_one_exact_serializable_lifecycle_effect() {
    let request = request(1);
    let plan = plan_lifecycle_wake_admission(LifecycleWakeAdmissionInput {
        subject: ActivitySubject::Episode {
            episode_id: request.reason_episode().unwrap(),
        },
        request: request.clone(),
    })
    .unwrap();
    let (_, _, _, facts, effects, commands, disposition) = plan.into_parts();
    assert_eq!(disposition, RequestDisposition::Accepted);
    assert_eq!(facts.len(), 2);
    assert!(commands.is_empty());
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0].request.execution,
        DurableEffectExecution::Lifecycle { request: exact } if exact == &request
    ));
    let json = serde_json::to_string(&effects[0].request).unwrap();
    assert_eq!(
        serde_json::from_str::<crate::DurableExternalEffectRequest>(&json).unwrap(),
        effects[0].request
    );
}

#[test]
fn failed_observation_can_atomically_authorize_only_the_next_exact_attempt() {
    let current = request(1);
    let retry = request(2);
    let plan = plan_lifecycle_wake_observation(LifecycleWakeObservationInput {
        identity_attempt_id: EffectAttemptId::from_parts(8, 1),
        effect_attempt_id: EffectAttemptId::from_parts(8, 1),
        intent_id: EffectIntentId::from_parts(9, 1),
        authorizing_activity_id: ActivityId::from_parts(10, 1),
        correlation_id: ActivityCorrelationId::from_parts(11, 1),
        subject: ActivitySubject::Episode {
            episode_id: current.reason_episode().unwrap(),
        },
        request: current,
        outcome: EffectOutcome::Failed {
            code: crate::ActivityFailureCode::PlatformFailure,
        },
        retry: Some(retry.clone()),
    })
    .unwrap();
    let (_, _, _, facts, effects, _, _) = plan.into_parts();
    assert_eq!(facts.len(), 3);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0].request.execution,
        DurableEffectExecution::Lifecycle { request: exact } if exact == &retry
    ));
}

trait TestReasonEpisode {
    fn reason_episode(&self) -> Option<EpisodeId>;
}

impl TestReasonEpisode for DurableLifecycleEffectRequest {
    fn reason_episode(&self) -> Option<EpisodeId> {
        match self.reason {
            CoreWakeReason::TranscriptRetry { episode_id, .. } => Some(episode_id),
            _ => None,
        }
    }
}
