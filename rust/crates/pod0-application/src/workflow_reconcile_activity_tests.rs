use pod0_domain::{CommandId, ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};

use crate::{
    ActivityDomain, ActivityFact, InternalCommandKind, WorkflowOpportunity,
    WorkflowOpportunityReason, WorkflowReconcileActivityInput, WorkflowReconcileIntent,
    WorkflowReconcilePlan, plan_workflow_reconcile_activity,
};

#[test]
fn reconciliation_authorizes_exact_typed_children_in_one_plan() {
    let episode_id = EpisodeId::from_bytes([3; 16]);
    let first = plan_workflow_reconcile_activity(input(episode_id)).unwrap();
    let replay = plan_workflow_reconcile_activity(input(episode_id)).unwrap();
    assert_eq!(first, replay);

    let (_, _, mutation, facts, effects, commands, _) = first.into_parts();
    assert!(effects.is_empty());
    assert_eq!(mutation.next_episode_offset, Some(32));
    assert_eq!(commands.len(), 3);
    assert!(matches!(
        commands[0].command.kind,
        InternalCommandKind::EnsurePublisherChapters
    ));
    assert!(matches!(
        commands[1].command.kind,
        InternalCommandKind::EnsureModelChapters { .. }
    ));
    assert!(matches!(
        commands[2].command.kind,
        InternalCommandKind::ContinueWorkflowReconciliation {
            episode_offset: 32,
            ..
        }
    ));
    assert_eq!(commands[0].command.target, ActivityDomain::Chapter);
    assert_eq!(facts.len(), 5);
    for command in &commands {
        assert!(
            matches!(facts.get(command.authorizing_fact_index).map(|value| value.fact),
            Some(ActivityFact::InternalCommandAuthorized { internal_command_id, .. })
                if internal_command_id == command.internal_command_id)
        );
    }
}

fn input(episode_id: EpisodeId) -> WorkflowReconcileActivityInput {
    WorkflowReconcileActivityInput {
        command_id: CommandId::from_parts(41, 1),
        current_revision: StateRevision::new(7),
        opportunity: WorkflowOpportunity {
            reason: WorkflowOpportunityReason::Foreground,
            observed_at: UnixTimestampMilliseconds::new(1_000),
            capability_snapshot_id: ContentDigest::from_bytes([8; 32]),
        },
        plan: WorkflowReconcilePlan {
            intents: vec![
                WorkflowReconcileIntent::EnsurePublisherChapters { episode_id },
                WorkflowReconcileIntent::EnsureModelChapters {
                    episode_id,
                    configured_model: "openai/gpt-4o-mini".to_owned(),
                },
            ],
            next_episode_offset: Some(32),
        },
        disposition: crate::RequestDisposition::Accepted,
    }
}
