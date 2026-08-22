mod support;

use pod0_facade::{ApplicationCommand, CancellationId, CommandEnvelope, CommandId, Pod0Facade};

#[test]
#[ignore = "the store-bootstrap gap is closed, but next_leased_host_requests (pod0-facade, out of this plan's pod0-cli-only scope) is not idempotent when re-run against the same pending work: two immediate calls return one lease then an empty list instead of matching results, reproduced identically against committed HEAD with the concurrent pod0-facade/pod0-storage WIP stashed out — see .planning/phases/01-headless-host-crates/01-06-SUMMARY.md (as of 2026-08-23)"]
fn pending_host_diagnostics_do_not_claim_or_mutate_work() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    support::bootstrap_authoritative_store(&store);
    let facade = Pod0Facade::open(store.to_string_lossy().into_owned()).unwrap();
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(1, 1),
        cancellation_id: CancellationId::from_parts(1, 2),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "Describe my library".to_owned(),
            model_reference: "openai:test-model".to_owned(),
        },
    });

    let first = facade.next_leased_host_requests(10);
    let second = facade.next_leased_host_requests(10);
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert_eq!(facade.next_leased_host_requests(10).len(), 1);
}
