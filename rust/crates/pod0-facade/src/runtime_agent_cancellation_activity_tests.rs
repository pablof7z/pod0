use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn cancellation_atomically_supersedes_a_claimed_effect_and_rejects_late_output() {
    let fixture = PlaybackFixture::new();
    let start = super::tests::start_command(9_005);
    fixture.facade.dispatch(start.clone());
    let model = fixture.facade.next_leased_host_requests(1).remove(0);
    let before = turn(&fixture.facade, start.command_id);
    let cancel = CommandEnvelope {
        command_id: CommandId::from_parts(9_005, 2),
        cancellation_id: CancellationId::from_parts(9_005, 3),
        expected_revision: None,
        command: ApplicationCommand::CancelAgentTurn {
            turn_id: before.turn_id,
            expected_turn_revision: before.revision,
        },
    };
    fixture.facade.dispatch(cancel.clone());
    assert_eq!(
        turn(&fixture.facade, start.command_id).stage,
        AgentTurnStage::Cancelled
    );
    assert!(fixture.facade.next_leased_host_requests(1).is_empty());

    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    let state: i64 = connection
        .query_row(
            "SELECT state_code FROM pod0_effect_intents WHERE intent_id=?1",
            [model.lease.intent_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, 4);
    let cancellation_facts: i64 = connection
        .query_row(
            "SELECT count(*) FROM pod0_activity_facts WHERE command_id=?1",
            [cancel.command_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cancellation_facts, 2);

    let mut observation = super::tests::observe(
        &model.request,
        HostObservation::Failed {
            code: HostFailureCode::TimedOut,
            safe_detail: None,
        },
    );
    observation.observed_at = model.lease.expires_at;
    assert!(matches!(
        fixture
            .facade
            .record_leased_host_observation(LeasedHostObservationEnvelope {
                lease: model.lease,
                observation,
            }),
        HostObservationReceipt::Rejected {
            reason: HostObservationRejection::StaleWorkflow,
            ..
        }
    ));
}

#[test]
fn stale_cancellation_is_durably_rejected_without_retiring_work() {
    let fixture = PlaybackFixture::new();
    let start = super::tests::start_command(9_006);
    fixture.facade.dispatch(start.clone());
    let before = turn(&fixture.facade, start.command_id);
    let cancel = CommandEnvelope {
        command_id: CommandId::from_parts(9_006, 2),
        cancellation_id: CancellationId::from_parts(9_006, 3),
        expected_revision: None,
        command: ApplicationCommand::CancelAgentTurn {
            turn_id: before.turn_id,
            expected_turn_revision: StateRevision::INITIAL,
        },
    };
    fixture.facade.dispatch(cancel.clone());
    assert_eq!(
        turn(&fixture.facade, start.command_id).stage,
        AgentTurnStage::AwaitingModel
    );
    assert_eq!(fixture.facade.next_leased_host_requests(1).len(), 1);
    let connection = rusqlite::Connection::open(&fixture.target).unwrap();
    let payload: String = connection
        .query_row(
            "SELECT payload_json FROM pod0_activity_facts WHERE command_id=?1",
            [cancel.command_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(payload.contains("RevisionConflict"));
}

fn turn(facade: &Pod0Facade, command_id: CommandId) -> AgentTurnProjection {
    let conversation_id = ConversationId::from_bytes(command_id.into_bytes());
    let Projection::AgentConversation { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversation { conversation_id },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected agent conversation");
    };
    value.turns.into_iter().next().expect("turn exists")
}
