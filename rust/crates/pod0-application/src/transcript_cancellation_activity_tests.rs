use pod0_domain::{
    CancellationId, CommandId, EpisodeId, HostRequestId, StateRevision, TranscriptWorkflowId,
};

use crate::{
    ActivityFact, ActivitySubject, CancellationEffectTarget, DomainTransitionKind,
    DurableEffectExecution, RequestDisposition, TranscriptCancellationActivityInput,
    TranscriptTransition, plan_transcript_cancellation,
};

#[test]
fn cancellation_atomically_authorizes_the_exact_target_request() {
    let episode_id = EpisodeId::from_parts(3, 4);
    let workflow_id = TranscriptWorkflowId::from_parts(5, 6);
    let target_request_id = HostRequestId::from_parts(8, 9);
    let cancellation_id = CancellationId::from_parts(10, 11);
    let plan = plan_transcript_cancellation(TranscriptCancellationActivityInput {
        command_id: CommandId::from_parts(1, 2),
        episode_id,
        workflow_id,
        workflow_revision: StateRevision::new(7),
        target: Some(CancellationEffectTarget {
            subject: ActivitySubject::TranscriptWorkflow { workflow_id },
            episode_id: Some(episode_id),
            host_request_id: target_request_id,
            cancellation_id,
        }),
    })
    .unwrap();

    assert_eq!(plan.disposition(), RequestDisposition::Accepted);
    let (_, expected, _, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(7));
    assert_eq!(effects.len(), 1);
    let DurableEffectExecution::Cancellation { request } = &effects[0].request.execution else {
        panic!("exact cancellation execution")
    };
    assert_eq!(request.target_request_id, target_request_id);
    assert_eq!(request.cancellation_id, cancellation_id);
    assert_eq!(request.issued_revision, StateRevision::new(7));
    assert!(commands.is_empty());
    assert!(matches!(
        facts.get(1).unwrap().fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Transcript(TranscriptTransition::Cancelled),
            previous_revision,
            committed_revision,
        } if previous_revision == StateRevision::new(7)
            && committed_revision == StateRevision::new(8)
    ));
}
