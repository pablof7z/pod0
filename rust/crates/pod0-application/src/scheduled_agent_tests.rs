use pod0_domain::{
    ContentDigest, GeneratedArtifactId, ScheduledAttemptId, ScheduledTaskId, StateRevision,
    UnixTimestampMilliseconds,
};
use sha2::Digest as _;

use super::*;

fn time(value: i64) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(value)
}

fn digest(value: u8) -> ContentDigest {
    ContentDigest::from_bytes([value; 32])
}

fn definition(next_run_at: i64) -> ScheduledTaskDefinition {
    let prompt = "Find the most useful ideas from my podcasts.".to_owned();
    ScheduledTaskDefinition {
        task_id: ScheduledTaskId::from_parts(4, 8),
        label: "Daily briefing".to_owned(),
        prompt_revision: scheduled_prompt_revision(&prompt).unwrap(),
        prompt,
        model_reference: "openrouter:test/model".to_owned(),
        interval_milliseconds: 86_400_000,
        created_at: time(1_000),
        last_run_at: None,
        next_run_at: time(next_run_at),
        revision: StateRevision::new(1),
    }
}

fn started(at: i64) -> ScheduledAgentAttemptPlan {
    let occurrence = reconcile_scheduled_occurrence(&definition(1_000), time(at))
        .unwrap()
        .unwrap();
    begin_scheduled_agent_attempt(&occurrence, time(at)).unwrap()
}

#[test]
fn occurrence_and_attempt_identity_are_deterministic_and_fenced() {
    let task = ScheduledTaskId::from_parts(3, 9);
    let first = scheduled_occurrence_id(task, time(1_000));
    assert_eq!(first, scheduled_occurrence_id(task, time(1_000)));
    assert_ne!(first, scheduled_occurrence_id(task, time(1_001)));
    assert_ne!(
        first,
        scheduled_occurrence_id(ScheduledTaskId::from_parts(3, 10), time(1_000))
    );
    assert_eq!(scheduled_attempt_id(first, 0), None);
    assert_eq!(
        scheduled_attempt_id(first, 1),
        scheduled_attempt_id(first, 1)
    );
    assert_ne!(
        scheduled_attempt_id(first, 1),
        scheduled_attempt_id(first, 2)
    );
}

#[test]
fn prompt_revision_is_content_derived_and_bounded() {
    assert_eq!(
        scheduled_prompt_revision("Prepare a briefing"),
        scheduled_prompt_revision("Prepare a briefing")
    );
    assert_ne!(
        scheduled_prompt_revision("Prepare a briefing"),
        scheduled_prompt_revision("Prepare another briefing")
    );
    assert_eq!(scheduled_prompt_revision("   "), None);
    assert_eq!(
        scheduled_prompt_revision(&"x".repeat(MAX_SCHEDULED_AGENT_PROMPT_BYTES + 1)),
        None
    );
}

#[test]
fn reconciliation_is_due_once_even_after_many_missed_periods() {
    let definition = definition(10_000);
    assert_eq!(
        reconcile_scheduled_occurrence(&definition, time(9_999)).unwrap(),
        None
    );
    let due = reconcile_scheduled_occurrence(&definition, time(10_000))
        .unwrap()
        .unwrap();
    let much_later = reconcile_scheduled_occurrence(&definition, time(900_000_000))
        .unwrap()
        .unwrap();
    assert_eq!(due.occurrence_id, much_later.occurrence_id);
    assert_eq!(due.stage, ScheduledAgentStage::Pending);
}

#[test]
fn attempt_plan_uses_injected_time_and_stable_fences() {
    let first = started(1_500);
    let second = started(1_500);
    assert_eq!(first, second);
    assert_eq!(first.state.attempt, 1);
    assert_eq!(first.state.request_id, Some(first.request_id));
    assert_eq!(first.request.attempt_id, first.state.attempt_id.unwrap());
    assert_eq!(
        first.deadline_at,
        time(1_500 + SCHEDULED_AGENT_HOST_DEADLINE_MILLISECONDS)
    );
    assert_eq!(first.request.context, Vec::new());
    assert_eq!(
        scheduled_generated_artifact_id(first.request.attempt_id),
        scheduled_generated_artifact_id(second.request.attempt_id)
    );
}

#[test]
fn raw_output_qualification_is_bounded_and_rust_owns_identity() {
    let plan = started(1_500);
    let qualified = qualify_scheduled_agent_completion(&plan.request, "Briefing ready").unwrap();
    let ScheduledAgentExecutionObservation::Completed {
        occurrence_id,
        attempt_id,
        artifact_id,
        output_digest,
        output_excerpt,
    } = qualified
    else {
        panic!("completed")
    };
    assert_eq!(occurrence_id, plan.request.occurrence_id);
    assert_eq!(attempt_id, plan.request.attempt_id);
    assert_eq!(artifact_id, scheduled_generated_artifact_id(attempt_id));
    assert_eq!(
        output_digest,
        ContentDigest::from_bytes(sha2::Sha256::digest(b"Briefing ready").into())
    );
    assert_eq!(output_excerpt, "Briefing ready");
    assert_eq!(
        qualify_scheduled_agent_completion(&plan.request, "  "),
        None
    );
    assert_eq!(
        qualify_scheduled_agent_completion(
            &plan.request,
            &"x".repeat(MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES + 1)
        ),
        None
    );
}

#[test]
fn accepted_and_completed_observations_are_fenced_and_idempotent() {
    let mut state = started(1_500).state;
    let attempt_id = state.attempt_id.unwrap();
    let accepted = ScheduledAgentExecutionObservation::Accepted {
        occurrence_id: state.occurrence_id,
        attempt_id,
        provider_operation_id: Some("provider-1".to_owned()),
    };
    assert_eq!(
        apply_scheduled_agent_observation(&mut state, &accepted, time(1_600)),
        ScheduledAgentTransition::Applied
    );
    assert_eq!(state.stage, ScheduledAgentStage::HostAccepted);
    assert_eq!(
        apply_scheduled_agent_observation(&mut state, &accepted, time(1_601)),
        ScheduledAgentTransition::IgnoredDuplicate
    );
    let conflicting_acceptance = ScheduledAgentExecutionObservation::Accepted {
        occurrence_id: state.occurrence_id,
        attempt_id,
        provider_operation_id: Some("provider-2".to_owned()),
    };
    assert_eq!(
        apply_scheduled_agent_observation(&mut state, &conflicting_acceptance, time(1_602)),
        ScheduledAgentTransition::RejectedInvalid
    );

    let stale = ScheduledAgentExecutionObservation::Cancelled {
        occurrence_id: state.occurrence_id,
        attempt_id: ScheduledAttemptId::from_parts(99, 99),
    };
    assert_eq!(
        apply_scheduled_agent_observation(&mut state, &stale, time(1_700)),
        ScheduledAgentTransition::IgnoredStale
    );

    let completed = ScheduledAgentExecutionObservation::Completed {
        occurrence_id: state.occurrence_id,
        attempt_id,
        artifact_id: scheduled_generated_artifact_id(attempt_id),
        output_digest: digest(8),
        output_excerpt: "Finished briefing".to_owned(),
    };
    assert_eq!(
        apply_scheduled_agent_observation(&mut state, &completed, time(2_000)),
        ScheduledAgentTransition::Applied
    );
    assert_eq!(state.stage, ScheduledAgentStage::Succeeded);
    assert_eq!(state.output_digest, Some(digest(8)));
    assert_eq!(
        apply_scheduled_agent_observation(&mut state, &completed, time(2_001)),
        ScheduledAgentTransition::IgnoredDuplicate
    );
    let conflicting_completion = ScheduledAgentExecutionObservation::Completed {
        occurrence_id: state.occurrence_id,
        attempt_id,
        artifact_id: GeneratedArtifactId::from_parts(2, 4),
        output_digest: digest(8),
        output_excerpt: "Finished briefing".to_owned(),
    };
    assert_eq!(
        apply_scheduled_agent_observation(&mut state, &conflicting_completion, time(2_002)),
        ScheduledAgentTransition::RejectedInvalid
    );
}

#[test]
fn completion_advances_the_exact_occurrence_once_from_completion_time() {
    let definition = definition(1_000);
    let mut occurrence = started(1_000).state;
    let completed = ScheduledAgentExecutionObservation::Completed {
        occurrence_id: occurrence.occurrence_id,
        attempt_id: occurrence.attempt_id.unwrap(),
        artifact_id: scheduled_generated_artifact_id(occurrence.attempt_id.unwrap()),
        output_digest: digest(9),
        output_excerpt: "Done".to_owned(),
    };
    assert_eq!(
        apply_scheduled_agent_observation(&mut occurrence, &completed, time(2_000)),
        ScheduledAgentTransition::Applied
    );
    let advanced =
        advance_scheduled_task_after_completion(&definition, &occurrence, time(2_000)).unwrap();
    assert_eq!(advanced.last_run_at, Some(time(2_000)));
    assert_eq!(advanced.next_run_at, time(2_000 + 86_400_000));
    assert!(advance_scheduled_task_after_completion(&advanced, &occurrence, time(2_000)).is_err());
}

#[test]
fn failures_block_retry_or_fail_permanently_by_shared_policy() {
    let mut blocked = started(1_000).state;
    let missing = ScheduledAgentExecutionObservation::Failed {
        occurrence_id: blocked.occurrence_id,
        attempt_id: blocked.attempt_id.unwrap(),
        code: ScheduledAgentFailureCode::MissingCredential,
        safe_detail: None,
        retry_after_milliseconds: None,
    };
    assert_eq!(
        apply_scheduled_agent_observation(&mut blocked, &missing, time(1_100)),
        ScheduledAgentTransition::Applied
    );
    assert_eq!(blocked.stage, ScheduledAgentStage::Blocked);
    assert!(blocked.projection().allowed_actions.can_retry);

    let mut retry = started(1_000).state;
    let network = ScheduledAgentExecutionObservation::Failed {
        occurrence_id: retry.occurrence_id,
        attempt_id: retry.attempt_id.unwrap(),
        code: ScheduledAgentFailureCode::Network,
        safe_detail: Some("network unavailable".to_owned()),
        retry_after_milliseconds: Some(4_000),
    };
    apply_scheduled_agent_observation(&mut retry, &network, time(2_000));
    assert_eq!(retry.stage, ScheduledAgentStage::RetryScheduled);
    assert_eq!(retry.not_before, Some(time(6_000)));
    let failed_revision = retry.revision;
    assert_eq!(
        apply_scheduled_agent_observation(&mut retry, &network, time(2_001)),
        ScheduledAgentTransition::IgnoredDuplicate
    );
    assert_eq!(retry.revision, failed_revision);
    assert_eq!(retry.not_before, Some(time(6_000)));
    assert_eq!(
        begin_scheduled_agent_attempt(&retry, time(5_999)),
        Err(ScheduledAgentPolicyError::NotReady)
    );
    assert_eq!(
        begin_scheduled_agent_attempt(&retry, time(6_000))
            .unwrap()
            .state
            .attempt,
        2
    );

    let mut permanent = started(1_000).state;
    let denied = ScheduledAgentExecutionObservation::Failed {
        occurrence_id: permanent.occurrence_id,
        attempt_id: permanent.attempt_id.unwrap(),
        code: ScheduledAgentFailureCode::PermissionDenied,
        safe_detail: None,
        retry_after_milliseconds: None,
    };
    apply_scheduled_agent_observation(&mut permanent, &denied, time(2_000));
    assert_eq!(permanent.stage, ScheduledAgentStage::FailedPermanent);
    assert!(!permanent.projection().allowed_actions.can_retry);
}
