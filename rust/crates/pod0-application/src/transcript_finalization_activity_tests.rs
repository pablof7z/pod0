use pod0_domain::{CommandId, EpisodeId, StateRevision, TranscriptWorkflowId};

use crate::{
    ActivityDomain, ActivityFact, InternalCommandKind, TranscriptFinalizationActivityInput,
    TranscriptTransition, plan_transcript_finalization,
};

#[test]
fn finalization_authorizes_exactly_one_typed_evidence_command() {
    let plan = plan_transcript_finalization(TranscriptFinalizationActivityInput {
        command_id: CommandId::from_parts(1, 1),
        episode_id: EpisodeId::from_parts(2, 1),
        workflow_id: TranscriptWorkflowId::from_parts(3, 1),
        workflow_revision: StateRevision::new(7),
    })
    .unwrap();
    let (_, expected, _, facts, effects, commands, _) = plan.into_parts();
    assert_eq!(expected, StateRevision::new(7));
    assert!(effects.is_empty());
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].command.kind,
        InternalCommandKind::BuildTranscriptEvidence
    );
    assert_eq!(commands[0].command.target, ActivityDomain::RecallKnowledge);
    assert!(facts.iter().any(|fact| matches!(
        fact.fact,
        ActivityFact::DomainTransition {
            kind: crate::DomainTransitionKind::Transcript(
                TranscriptTransition::SelectionChanged
            ),
            committed_revision,
            ..
        } if committed_revision == StateRevision::new(8)
    )));
}
