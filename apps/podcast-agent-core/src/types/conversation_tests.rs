use super::*;


#[test]
fn turn_round_trips_through_serde() {
    let t = NostrConversationTurn {
        id: Uuid::nil(),
        role: ConversationRole::Assistant,
        content: "hello".into(),
        timestamp: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        metadata: None,
    };
    let j = serde_json::to_string(&t).expect("encode");
    let d: NostrConversationTurn = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, t);
}

#[test]
fn turn_role_serializes_snake_case() {
    let t = NostrConversationTurn::new(ConversationRole::User, "hi");
    let j = serde_json::to_string(&t).expect("encode");
    assert!(j.contains("\"role\":\"user\""));
}

#[test]
fn turn_with_metadata_round_trips() {
    let meta = TurnMetadata {
        provider: Some("openrouter".into()),
        model: Some("anthropic/claude".into()),
        tokens: Some(123),
        extra: serde_json::json!({"latency_ms": 42}),
    };
    let t = NostrConversationTurn::new(ConversationRole::Assistant, "ok")
        .with_metadata(meta.clone());
    let j = serde_json::to_string(&t).expect("encode");
    let d: NostrConversationTurn = serde_json::from_str(&j).expect("decode");
    assert_eq!(d.metadata, Some(meta));
}

#[test]
fn conversation_push_advances_updated_at() {
    let mut c = NostrConversation::new();
    let ts = DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap();
    let turn = NostrConversationTurn {
        id: Uuid::nil(),
        role: ConversationRole::User,
        content: "yo".into(),
        timestamp: ts,
        metadata: None,
    };
    c.push(turn);
    assert_eq!(c.turns.len(), 1);
    assert_eq!(c.updated_at, ts);
}

#[test]
fn conversation_omits_none_title() {
    let c = NostrConversation::new();
    let j = serde_json::to_string(&c).expect("encode");
    assert!(!j.contains("\"title\""));
}
