use std::path::PathBuf;

use pod0_application::{
    AgentModelObservation, AgentModelUsageObservation, AgentToolAction, AgentTurnStart,
    AgentTurnState, AgentWorkflowAcceptance,
};
use pod0_domain::{
    AgentExecutionFenceId, AgentTurnId, CommandId, ConversationId, StateRevision,
    UnixTimestampMilliseconds,
};
use tempfile::TempDir;

use crate::{
    AgentAuditKind, AgentCommandContext, AgentMutationOutcome, AgentStore, AgentTurnMutation,
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, MigrationClock, StorageError,
};

struct FixedClock(i64);
impl MigrationClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        self.0
    }
}

struct Fixture {
    _directory: TempDir,
    path: PathBuf,
    store: AgentStore,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("core.sqlite3");
        CoreStoreMigrator::new(FixedClock(1_000))
            .migrate(
                &path,
                CURRENT_SCHEMA_VERSION,
                &directory.path().join("backup.sqlite3"),
                command(1),
            )
            .unwrap();
        let store = AgentStore::open(&path).unwrap();
        Self {
            _directory: directory,
            path,
            store,
        }
    }
}

fn command(value: u8) -> CommandId {
    CommandId::from_bytes([value; 16])
}

fn context(value: u8, observed_at: i64) -> AgentCommandContext {
    AgentCommandContext {
        command_id: command(value),
        command_fingerprint: [value; 32],
        observed_at: UnixTimestampMilliseconds::new(observed_at),
    }
}

fn state(seed: u8) -> AgentTurnState {
    state_for(
        seed,
        ConversationId::from_bytes([9; 16]),
        "Save architecture matters as a note",
        10,
    )
}

fn state_for(
    seed: u8,
    conversation_id: ConversationId,
    user_input: &str,
    observed_at: i64,
) -> AgentTurnState {
    AgentTurnState::start(AgentTurnStart {
        conversation_id,
        turn_id: AgentTurnId::from_bytes([seed; 16]),
        model_fence_id: AgentExecutionFenceId::from_bytes([seed + 1; 16]),
        user_input: user_input.into(),
        model_reference: "openrouter/test".into(),
        available_tools: vec![pod0_application::AgentToolName::CreateNote],
        cancellation_id: pod0_domain::CancellationId::from_parts(9, seed.into()),
        observed_at: UnixTimestampMilliseconds::new(observed_at),
    })
    .unwrap()
}

#[test]
fn start_and_command_replay_are_idempotent_and_conflicting_reuse_fails() {
    let fixture = Fixture::new();
    let state = state(2);
    assert!(matches!(
        fixture.store.start_turn(context(2, 10), &state).unwrap(),
        AgentMutationOutcome::Applied(_)
    ));
    assert!(matches!(
        fixture.store.start_turn(context(2, 10), &state).unwrap(),
        AgentMutationOutcome::Duplicate(_)
    ));
    let mut conflict = context(2, 10);
    conflict.command_fingerprint = [99; 32];
    assert_eq!(
        fixture.store.start_turn(conflict, &state).unwrap_err(),
        StorageError::AgentCommandConflict
    );
}

#[test]
fn persisted_turn_rehydrates_after_process_restart() {
    let fixture = Fixture::new();
    let mut state = state(3);
    fixture.store.start_turn(context(3, 10), &state).unwrap();
    assert_eq!(
        state.observe_model(AgentModelObservation {
            turn_id: state.projection().turn_id,
            model_fence_id: AgentExecutionFenceId::from_bytes([4; 16]),
            assistant_text: "I'll save that.".into(),
            proposed_action: Some(AgentToolAction::CreateNote {
                text: "Architecture matters".into(),
            }),
            usage: Some(AgentModelUsageObservation {
                prompt_tokens: 80,
                completion_tokens: 12,
                cached_prompt_tokens: Some(20),
            }),
            observed_at: UnixTimestampMilliseconds::new(20),
        }),
        AgentWorkflowAcceptance::Updated
    );
    fixture
        .store
        .update_turn(
            context(4, 20),
            AgentTurnMutation {
                expected_revision: StateRevision::new(1),
                audit_kind: AgentAuditKind::ModelObserved,
            },
            &state,
        )
        .unwrap();
    let reopened = AgentStore::open(&fixture.path).unwrap();
    let recovered = reopened.turn(state.projection().turn_id).unwrap().unwrap();
    assert_eq!(recovered.projection().model_usage.len(), 1);
    assert_eq!(recovered.projection().model_usage[0].prompt_tokens, 80);
    assert_eq!(recovered, state);
    assert!(recovered.is_valid_for_recovery());
}

#[test]
fn stale_revision_cannot_overwrite_newer_turn_state() {
    let fixture = Fixture::new();
    let mut state = state(4);
    fixture.store.start_turn(context(5, 10), &state).unwrap();
    assert_eq!(
        state.cancel(UnixTimestampMilliseconds::new(20)),
        AgentWorkflowAcceptance::Updated
    );
    let error = fixture
        .store
        .update_turn(
            context(6, 20),
            AgentTurnMutation {
                expected_revision: StateRevision::new(99),
                audit_kind: AgentAuditKind::Cancelled,
            },
            &state,
        )
        .unwrap_err();
    assert_eq!(error, StorageError::AgentTurnConflict);
    assert_eq!(
        fixture
            .store
            .turn(state.projection().turn_id)
            .unwrap()
            .unwrap()
            .projection()
            .revision,
        StateRevision::new(1)
    );
}

#[test]
fn corrupt_state_payload_fails_closed() {
    let fixture = Fixture::new();
    let state = state(5);
    fixture.store.start_turn(context(7, 10), &state).unwrap();
    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    connection
        .execute(
            "UPDATE pod0_agent_turns SET state_json=?1 WHERE turn_id=?2",
            rusqlite::params![
                b"{}".as_slice(),
                state.projection().turn_id.into_bytes().as_slice()
            ],
        )
        .unwrap();
    assert!(matches!(
        fixture.store.turn(state.projection().turn_id),
        Err(StorageError::CorruptSchema { .. })
    ));
}

#[test]
fn conversation_page_is_bounded_and_reports_more() {
    let fixture = Fixture::new();
    for seed in 10..13 {
        fixture
            .store
            .start_turn(context(seed, i64::from(seed)), &state(seed))
            .unwrap();
    }
    let page = fixture
        .store
        .turn_page(ConversationId::from_bytes([9; 16]), 0, 2)
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(page.has_more);
}

#[test]
fn conversation_index_is_bounded_ordered_and_survives_restart() {
    let fixture = Fixture::new();
    let older_id = ConversationId::from_bytes([31; 16]);
    let newer_id = ConversationId::from_bytes([32; 16]);
    fixture
        .store
        .start_turn(
            context(31, 100),
            &state_for(31, older_id, "Older question", 100),
        )
        .unwrap();
    fixture
        .store
        .start_turn(
            context(32, 200),
            &state_for(32, newer_id, "Newest question", 200),
        )
        .unwrap();

    let first_page = fixture.store.conversation_page(0, 1).unwrap();
    assert_eq!(first_page.items.len(), 1);
    assert!(first_page.has_more);
    assert_eq!(first_page.items[0].conversation_id, newer_id);
    assert_eq!(first_page.items[0].title, "Newest question");
    assert_eq!(first_page.items[0].preview, "Newest question");
    assert_eq!(first_page.items[0].turn_count, 1);

    let reopened = AgentStore::open(&fixture.path).unwrap();
    let second_page = reopened.conversation_page(1, 1).unwrap();
    assert_eq!(second_page.items.len(), 1);
    assert!(!second_page.has_more);
    assert_eq!(second_page.items[0].conversation_id, older_id);
}
