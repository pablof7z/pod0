use super::*;


#[test]
fn action_ids_match_documented_strings() {
    assert_eq!(ACTION_AGENT_SEND, "podcast.agent.send");
    assert_eq!(ACTION_AGENT_APPROVE, "podcast.agent.approve");
    assert_eq!(ACTION_AGENT_DENY, "podcast.agent.deny");
    assert_eq!(ACTION_AGENT_CLEAR, "podcast.agent.clear");
}

#[test]
fn send_action_round_trips_with_conversation() {
    let a = SendAgentMessageAction {
        conversation_id: Some("conv-1".into()),
        message: "hi".into(),
    };
    let j = serde_json::to_string(&a).expect("encode");
    let d: SendAgentMessageAction = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, a);
}

#[test]
fn send_action_omits_none_conversation_id() {
    let a = SendAgentMessageAction {
        conversation_id: None,
        message: "hi".into(),
    };
    let j = serde_json::to_string(&a).expect("encode");
    assert_eq!(j, r#"{"message":"hi"}"#);
    let d: SendAgentMessageAction = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, a);
}

#[test]
fn approve_action_round_trips() {
    let a = ApproveAction {
        approval_id: "ap-1".into(),
    };
    let j = serde_json::to_string(&a).expect("encode");
    assert_eq!(j, r#"{"approval_id":"ap-1"}"#);
    let d: ApproveAction = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, a);
}

#[test]
fn deny_action_round_trips_with_and_without_reason() {
    let with = DenyAction {
        approval_id: "ap-1".into(),
        reason: Some("nope".into()),
    };
    let j = serde_json::to_string(&with).expect("encode");
    assert_eq!(j, r#"{"approval_id":"ap-1","reason":"nope"}"#);
    let d: DenyAction = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, with);

    let without = DenyAction {
        approval_id: "ap-1".into(),
        reason: None,
    };
    let j = serde_json::to_string(&without).expect("encode");
    assert_eq!(j, r#"{"approval_id":"ap-1"}"#);
    let d: DenyAction = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, without);
}

#[test]
fn clear_action_round_trips() {
    let a = ClearConversationAction {
        conversation_id: "conv-7".into(),
    };
    let j = serde_json::to_string(&a).expect("encode");
    assert_eq!(j, r#"{"conversation_id":"conv-7"}"#);
    let d: ClearConversationAction = serde_json::from_str(&j).expect("decode");
    assert_eq!(d, a);
}
