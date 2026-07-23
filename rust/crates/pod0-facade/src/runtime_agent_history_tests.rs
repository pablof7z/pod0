use std::collections::HashSet;

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

fn start_command(id: u64) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(101, id),
        cancellation_id: CancellationId::from_parts(102, id),
        expected_revision: None,
        command: ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "Save architecture matters as a note".to_owned(),
            model_reference: "openrouter/test".to_owned(),
        },
    }
}

fn conversations(facade: &Pod0Facade, offset: u32, max_items: u16) -> AgentConversationsProjection {
    let Projection::AgentConversations { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::AgentConversations,
            offset,
            max_items,
        })
        .projection
    else {
        panic!("expected agent conversations projection");
    };
    value
}

#[test]
fn conversation_index_pages_durable_rust_history_after_restart() {
    let fixture = PlaybackFixture::new();
    let first = start_command(41);
    let second = start_command(42);
    fixture.facade.dispatch(first.clone());
    fixture.facade.dispatch(second.clone());

    let page = conversations(&fixture.facade, 0, 1);
    assert!(page.failure.is_none());
    assert_eq!(page.conversations.len(), 1);
    assert!(page.has_more);
    assert_eq!(page.conversations[0].turn_count, 1);
    assert_eq!(
        page.conversations[0].title,
        "Save architecture matters as a note"
    );

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let all = conversations(&reopened, 0, 10);
    assert!(all.failure.is_none());
    assert_eq!(all.conversations.len(), 2);
    assert!(!all.has_more);
    let ids = all
        .conversations
        .iter()
        .map(|summary| summary.conversation_id)
        .collect::<HashSet<_>>();
    assert_eq!(
        ids,
        HashSet::from([
            ConversationId::from_bytes(first.command_id.into_bytes()),
            ConversationId::from_bytes(second.command_id.into_bytes()),
        ])
    );
}
