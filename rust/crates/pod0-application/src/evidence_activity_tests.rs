use pod0_domain::{
    ActivityCorrelationId, ActivityId, CancellationId, CommandId, HostRequestId,
    InternalCommandId, StateRevision, TranscriptWorkflowId, UnixTimestampMilliseconds,
};

use crate::{
    ActivityFact, DurableEvidenceEmbeddingEffectRequest, DurableEvidenceIndexCompletion,
    EvidenceAdmissionActivityInput, ExternalEffectKind, RecallEmbeddingInput,
    plan_evidence_admission,
};

#[test]
fn evidence_admission_is_caused_by_the_internal_command_authorization() {
    let artifact = crate::build_evidence_artifact(
        &crate::knowledge_test_fixture::golden_input(),
        crate::EvidenceChunkPolicy::default(),
    )
    .unwrap();
    let cause = ActivityId::from_parts(2, 1);
    let plan = plan_evidence_admission(EvidenceAdmissionActivityInput {
        internal_command_id: InternalCommandId::from_parts(1, 1),
        authorizing_activity_id: cause,
        correlation_id: ActivityCorrelationId::from_parts(3, 1),
        episode_id: artifact.version.episode_id,
        effect: Some(DurableEvidenceEmbeddingEffectRequest {
            request_id: HostRequestId::from_parts(4, 1),
            command_id: CommandId::from_parts(5, 1),
            cancellation_id: CancellationId::from_parts(6, 1),
            issued_revision: StateRevision::new(7),
            deadline_at: UnixTimestampMilliseconds::new(10_000),
            episode_id: artifact.version.episode_id,
            generation_id: artifact.generation_id,
            expected_span_count: u32::try_from(artifact.spans.len()).unwrap(),
            provider: pod0_domain::RecallEmbeddingProvider::OpenRouter,
            model: "fixture".into(),
            spans: artifact
                .spans
                .iter()
                .map(|span| RecallEmbeddingInput {
                    span_id: span.span_id,
                    text: span.text.clone(),
                })
                .collect(),
            completion: DurableEvidenceIndexCompletion::TranscriptWorkflow {
                workflow_id: TranscriptWorkflowId::from_parts(8, 1),
                input_version: "fixture-v1".into(),
            },
        }),
        artifact,
    })
    .unwrap();
    let (_, _, _, facts, effects, commands, _) = plan.into_parts();
    assert!(
        facts
            .iter()
            .all(|fact| fact.caused_by_activity_id == Some(cause))
    );
    assert!(facts.iter().any(|fact| matches!(
        fact.fact,
        ActivityFact::EffectAuthorized {
            kind: ExternalEffectKind::RecallProvider,
            ..
        }
    )));
    assert_eq!(effects.len(), 1);
    assert!(commands.is_empty());
}
