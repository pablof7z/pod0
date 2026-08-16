use pod0_domain::{CommandId, PublicationId, StateRevision};

use crate::{
    ActivityFact, DurableEffectExecution, ExternalEffectKind, Pod0PublicationDraft,
    PublicationPrepareActivityInput, RequestDisposition, plan_publication_prepare,
};

#[test]
fn publication_prepare_authorizes_the_exact_nmp_draft_but_keeps_content_out_of_journal() {
    let draft = Pod0PublicationDraft {
        publication_id: PublicationId::from_parts(1, 2),
        expected_author_hex: "01".repeat(32),
        correlation_token: "correlation".into(),
        created_at_seconds: 10,
        kind: 30_075,
        tags: vec![vec!["d".into(), "episode".into()]],
        content: "private generated description".into(),
    };
    let plan = plan_publication_prepare(PublicationPrepareActivityInput {
        command_id: CommandId::from_parts(3, 4),
        current_revision: StateRevision::INITIAL,
        committed_revision: StateRevision::new(1),
        disposition: RequestDisposition::Accepted,
        draft: Some(draft.clone()),
    })
    .unwrap();
    let (_, _, _, facts, effects, _, _) = plan.into_parts();
    assert!(facts.iter().any(|fact| matches!(
        fact.fact,
        ActivityFact::EffectAuthorized {
            kind: ExternalEffectKind::Publication,
            ..
        }
    )));
    assert!(matches!(
        &effects[0].request.execution,
        DurableEffectExecution::Publication { draft: exact } if exact == &draft
    ));
    let journal = serde_json::to_string(&facts.into_vec()).unwrap();
    assert!(!journal.contains("private generated description"));
    assert!(!journal.contains(&draft.expected_author_hex));
}
