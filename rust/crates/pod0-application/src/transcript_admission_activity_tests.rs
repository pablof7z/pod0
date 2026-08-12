use pod0_domain::{
    ActivityCorrelationId, ActivityId, CommandId, EpisodeId, InternalCommandId, StateRevision,
    TranscriptWorkflowId,
};

use crate::{
    ActivityFact, RequestDisposition, TranscriptAdmissionActivityInput,
    TranscriptAdmissionMutation, TranscriptDispositionActivityInput,
    TranscriptInternalAdmissionActivityInput, TranscriptWorkflowOrigin, plan_transcript_admission,
    plan_transcript_internal_admission, plan_transcript_request_disposition,
};

#[test]
fn transcript_admission_records_transition_or_duplicate_without_faking_state() {
    let base = TranscriptAdmissionActivityInput {
        command_id: CommandId::from_parts(1, 2),
        episode_id: EpisodeId::from_parts(3, 4),
        workflow_id: TranscriptWorkflowId::from_parts(5, 6),
        current_workflow_revision: Some(StateRevision::new(7)),
        exact_replay: false,
        origin: TranscriptWorkflowOrigin::User,
    };
    let accepted = plan_transcript_admission(base).unwrap();
    assert_eq!(accepted.disposition(), RequestDisposition::Accepted);
    let (_, _, mutation, facts, _, _, _) = accepted.into_parts();
    assert_eq!(mutation, TranscriptAdmissionMutation::Ensure);
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition { .. }
    ));

    let duplicate = plan_transcript_admission(TranscriptAdmissionActivityInput {
        exact_replay: true,
        ..base
    })
    .unwrap();
    assert_eq!(duplicate.disposition(), RequestDisposition::Duplicate);
    assert_eq!(duplicate.into_parts().3.len(), 1);
}

#[test]
fn internal_transcript_admission_preserves_authorization_causation() {
    let cause = ActivityId::from_parts(21, 22);
    let correlation = ActivityCorrelationId::from_parts(23, 24);
    let plan = plan_transcript_internal_admission(TranscriptInternalAdmissionActivityInput {
        internal_command_id: InternalCommandId::from_parts(25, 26),
        authorizing_activity_id: cause,
        correlation_id: correlation,
        episode_id: EpisodeId::from_parts(27, 28),
        workflow_id: TranscriptWorkflowId::from_parts(29, 30),
        current_workflow_revision: None,
        exact_replay: false,
    })
    .unwrap();
    let (_, _, _, facts, _, _, _) = plan.into_parts();
    assert!(facts.iter().all(|fact| {
        fact.origin == crate::ActivityOrigin::InternalCommand
            && fact.caused_by_activity_id == Some(cause)
            && fact.correlation_id == correlation
            && fact.command_id.is_none()
    }));
}

#[test]
fn terminal_request_disposition_has_no_fake_transition() {
    let plan = plan_transcript_request_disposition(TranscriptDispositionActivityInput {
        command_id: CommandId::from_parts(11, 12),
        episode_id: EpisodeId::from_parts(13, 14),
        state_revision: StateRevision::new(15),
        origin: TranscriptWorkflowOrigin::Automatic,
        disposition: RequestDisposition::AlreadyComplete,
    })
    .unwrap();
    assert_eq!(plan.disposition(), RequestDisposition::AlreadyComplete);
    assert_eq!(plan.into_parts().3.len(), 1);
}
