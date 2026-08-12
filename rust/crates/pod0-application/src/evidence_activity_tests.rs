use pod0_domain::{
    ActivityCorrelationId, ActivityId, InternalCommandId, UnixTimestampMilliseconds,
};

use crate::{
    ActivityFact, EvidenceAdmissionActivityInput, ExternalEffectKind, plan_evidence_admission,
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
        artifact,
        deadline_at: UnixTimestampMilliseconds::new(10_000),
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
