use pod0_domain::{CommandId, EpisodeId, StateRevision};

use crate::{
    ActivityFact, ActivitySubject, EvidenceRebuildActivityInput, EvidenceRebuildMutation,
    RequestDisposition, plan_evidence_rebuild,
};

#[test]
fn changed_evidence_is_episode_scoped_and_advances_once() {
    let episode_id = EpisodeId::from_parts(7, 8);
    let plan = plan_evidence_rebuild(EvidenceRebuildActivityInput {
        command_id: CommandId::from_parts(1, 2),
        episode_id,
        current_revision: StateRevision::new(11),
        semantic_change: true,
        effect: None,
    })
    .unwrap();

    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, EvidenceRebuildMutation::Apply);
    assert_eq!(facts.len(), 2);
    assert!(facts.iter().all(|fact| {
        fact.subject == ActivitySubject::Episode { episode_id }
            && fact.episode_id == Some(episode_id)
    }));
    assert!(matches!(
        facts.get(1).expect("transition fact").fact,
        ActivityFact::DomainTransition {
            previous_revision: StateRevision { value: 11 },
            committed_revision: StateRevision { value: 12 },
            ..
        }
    ));
}

#[test]
fn current_generation_records_a_no_change_disposition() {
    let plan = plan_evidence_rebuild(EvidenceRebuildActivityInput {
        command_id: CommandId::from_parts(3, 4),
        episode_id: EpisodeId::from_parts(9, 10),
        current_revision: StateRevision::new(4),
        semantic_change: false,
        effect: None,
    })
    .unwrap();

    let (_, _, mutation, facts, _, _, _) = plan.into_parts();
    assert_eq!(mutation, EvidenceRebuildMutation::None);
    assert_eq!(facts.len(), 1);
    assert_eq!(
        facts.get(0).expect("disposition fact").fact,
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::NoSemanticChange,
        }
    );
}
