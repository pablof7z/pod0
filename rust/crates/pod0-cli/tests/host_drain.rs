use pod0_facade::{ApplicationCommand, CancellationId, CommandEnvelope, CommandId, Pod0Facade};

#[test]
#[ignore = "requires Pod0Facade store-bootstrap support not yet committed to pod0-storage/pod0-facade — see .planning/phases/01-headless-host-crates/01-VERIFICATION.md; un-ignore once that lands (as of 2026-08-22)"]
fn pending_host_diagnostics_do_not_claim_or_mutate_work() {
    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
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
