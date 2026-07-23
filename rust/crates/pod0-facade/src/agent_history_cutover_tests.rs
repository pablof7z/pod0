use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

fn digest(seed: u8) -> ContentDigest {
    ContentDigest::from_bytes([seed; 32])
}

fn conversation(seed: u8) -> LegacyAgentHistoryConversationInput {
    LegacyAgentHistoryConversationInput {
        conversation_id: ConversationId::from_bytes([seed; 16]),
        title: "Imported title".into(),
        created_at: UnixTimestampMilliseconds::new(100),
        updated_at: UnixTimestampMilliseconds::new(300),
        turns: vec![
            LegacyAgentHistoryTurnInput {
                turn_id: AgentTurnId::from_bytes([seed + 1; 16]),
                created_at: UnixTimestampMilliseconds::new(100),
                updated_at: UnixTimestampMilliseconds::new(200),
                messages: vec![
                    LegacyAgentHistoryMessageInput {
                        role: AgentMessageRole::User,
                        content: "What mattered?".into(),
                    },
                    LegacyAgentHistoryMessageInput {
                        role: AgentMessageRole::Assistant,
                        content: "Architecture mattered.".into(),
                    },
                ],
            },
            LegacyAgentHistoryTurnInput {
                turn_id: AgentTurnId::from_bytes([seed + 2; 16]),
                created_at: UnixTimestampMilliseconds::new(250),
                updated_at: UnixTimestampMilliseconds::new(300),
                messages: vec![
                    LegacyAgentHistoryMessageInput {
                        role: AgentMessageRole::User,
                        content: "Try again".into(),
                    },
                    LegacyAgentHistoryMessageInput {
                        role: AgentMessageRole::Error,
                        content: "Provider unavailable".into(),
                    },
                ],
            },
        ],
    }
}

#[test]
fn staged_history_survives_restart_and_commits_once() {
    let fixture = PlaybackFixture::new();
    let input = vec![conversation(20)];
    let inspected =
        fixture
            .facade
            .inspect_legacy_agent_history_cutover(digest(1), 400, input.clone());
    assert_eq!(inspected.stage, LegacyAgentHistoryCutoverStage::NotStarted);
    assert_eq!(inspected.conversation_count, 1);
    assert_eq!(inspected.turn_count, 2);
    assert_eq!(inspected.message_count, 4);

    let staged = fixture
        .facade
        .stage_legacy_agent_history_cutover(digest(1), 400, input.clone());
    assert_eq!(staged.stage, LegacyAgentHistoryCutoverStage::Staged);
    let generation = staged.source_generation.unwrap();
    let duplicate = fixture
        .facade
        .stage_legacy_agent_history_cutover(digest(1), 400, input);
    assert_eq!(duplicate, staged);

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let verified = reopened.verify_legacy_agent_history_cutover(generation);
    assert_eq!(verified.stage, LegacyAgentHistoryCutoverStage::Verified);
    drop(reopened);

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let committed = reopened.commit_legacy_agent_history_cutover(generation);
    assert_eq!(
        committed.stage,
        LegacyAgentHistoryCutoverStage::Authoritative
    );
    assert_eq!(
        reopened.commit_legacy_agent_history_cutover(generation),
        committed
    );

    let Projection::AgentConversations { value } = reopened
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversations,
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected agent conversation history");
    };
    assert_eq!(value.conversations.len(), 1);
    assert_eq!(value.conversations[0].title, "Imported title");
    assert_eq!(value.conversations[0].turn_count, 2);

    let Projection::AgentConversation { value } = reopened
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversation {
                conversation_id: ConversationId::from_bytes([20; 16]),
            },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected imported conversation");
    };
    assert_eq!(value.turns.len(), 2);
    assert_eq!(value.turns[0].stage, AgentTurnStage::Failed);
    assert_eq!(
        value.turns[0].messages.last().unwrap().role,
        AgentMessageRole::Error
    );
}

#[test]
fn empty_history_can_commit_authority() {
    let fixture = PlaybackFixture::new();
    let staged = fixture
        .facade
        .stage_legacy_agent_history_cutover(digest(2), 2, Vec::new());
    let generation = staged.source_generation.unwrap();
    assert_eq!(
        fixture
            .facade
            .verify_legacy_agent_history_cutover(generation)
            .stage,
        LegacyAgentHistoryCutoverStage::Verified
    );
    assert_eq!(
        fixture
            .facade
            .commit_legacy_agent_history_cutover(generation)
            .stage,
        LegacyAgentHistoryCutoverStage::Authoritative
    );
}

#[test]
fn malformed_or_conflicting_history_fails_closed() {
    let fixture = PlaybackFixture::new();
    let mut malformed = conversation(30);
    malformed.turns[0].messages[0].role = AgentMessageRole::Assistant;
    let blocked =
        fixture
            .facade
            .inspect_legacy_agent_history_cutover(digest(3), 100, vec![malformed]);
    assert_eq!(blocked.stage, LegacyAgentHistoryCutoverStage::Blocked);
    assert_eq!(
        blocked.failure.unwrap().code,
        LegacyAgentHistoryCutoverFailureCode::InvalidSource
    );

    let active = CommandEnvelope {
        command_id: CommandId::from_bytes([31; 16]),
        cancellation_id: CancellationId::from_bytes([32; 16]),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: Some(ConversationId::from_bytes([30; 16])),
            user_input: "Active turn".into(),
            model_reference: "openrouter/test".into(),
        },
    };
    fixture.facade.dispatch(active);
    let input = vec![conversation(30)];
    let staged = fixture
        .facade
        .stage_legacy_agent_history_cutover(digest(4), 100, input);
    let blocked = fixture
        .facade
        .verify_legacy_agent_history_cutover(staged.source_generation.unwrap());
    assert_eq!(blocked.stage, LegacyAgentHistoryCutoverStage::Blocked);
    assert_eq!(
        blocked.failure.unwrap().code,
        LegacyAgentHistoryCutoverFailureCode::ConflictingCoreState
    );
}

#[test]
fn staged_history_can_be_discarded_before_authority() {
    let fixture = PlaybackFixture::new();
    let staged =
        fixture
            .facade
            .stage_legacy_agent_history_cutover(digest(5), 100, vec![conversation(40)]);
    let discarded = fixture
        .facade
        .discard_staged_legacy_agent_history_cutover(staged.source_generation.unwrap());
    assert_eq!(discarded.stage, LegacyAgentHistoryCutoverStage::NotStarted);
    assert_eq!(
        fixture.facade.agent_history_cutover().stage,
        LegacyAgentHistoryCutoverStage::NotStarted
    );
}
