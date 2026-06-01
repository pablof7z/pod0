use super::*;
#[test]
fn action_ids_match_documented_strings() {
    assert_eq!(
        ACTION_PUBLISH_CREATE_SYNTHETIC,
        "podcast.publish.create_synthetic_podcast"
    );
    assert_eq!(
        ACTION_PUBLISH_UPDATE_OWNED,
        "podcast.publish.update_owned_podcast"
    );
    assert_eq!(
        ACTION_PUBLISH_DELETE_OWNED,
        "podcast.publish.delete_owned_podcast"
    );
    assert_eq!(ACTION_PUBLISH_CREATE_OWNED, "podcast.publish.create_owned_podcast");
    assert_eq!(ACTION_PUBLISH_PUBLISH_SHOW, "podcast.publish.publish_show");
    assert_eq!(ACTION_PUBLISH_PUBLISH_EPISODE, "podcast.publish.publish_episode");
    assert_eq!(
        ACTION_PUBLISH_PUBLISH_AUTHOR_CLAIM,
        "podcast.publish.publish_author_claim"
    );
    assert_eq!(ACTION_PUBLISH_REMOVE_OWNED, "podcast.publish.remove_owned_podcast");
}
#[test]
fn create_owned_podcast_round_trips() {
    let a = PublishAction::CreateOwnedPodcast {
        podcast_id: "pod-7".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"create_owned_podcast""#));
    assert!(json.contains(r#""podcast_id":"pod-7""#));
    let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn publish_show_round_trips() {
    let a = PublishAction::PublishShow {
        podcast_id: "pod-7".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"publish_show""#));
    let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn publish_episode_round_trips() {
    let a = PublishAction::PublishEpisode {
        episode_id: "ep-7".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"publish_episode""#));
    assert!(json.contains(r#""episode_id":"ep-7""#));
    let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn publish_author_claim_round_trips() {
    let a = PublishAction::PublishAuthorClaim {
        agent_pubkey_hex: "deadbeef".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"publish_author_claim""#));
    assert!(json.contains(r#""agent_pubkey_hex":"deadbeef""#));
    let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn remove_owned_podcast_round_trips() {
    let a = PublishAction::RemoveOwnedPodcast {
        podcast_id: "pod-7".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"remove_owned_podcast""#));
    let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}
#[test]
fn create_synthetic_podcast_round_trips() {
    let a = PublishAction::CreateSyntheticPodcast {
        podcast_id: "pod-9".into(),
        title: "Synth Show".into(),
        description: "desc".into(),
        author: "Agent".into(),
        artwork_url: Some("https://img".into()),
        language: Some("en".into()),
        categories: vec!["Tech".into()],
        visibility: Some("public".into()),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"create_synthetic_podcast""#));
    assert!(json.contains(r#""podcast_id":"pod-9""#));
    assert!(json.contains(r#""title":"Synth Show""#));
    let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}

#[test]
fn create_synthetic_podcast_decodes_minimal_body() {
    // Swift may omit optional/default fields; decode must tolerate that.
    let json = r#"{"op":"create_synthetic_podcast","podcast_id":"p","title":"T"}"#;
    let decoded: PublishAction = serde_json::from_str(json).expect("decode");
    assert_eq!(
        decoded,
        PublishAction::CreateSyntheticPodcast {
            podcast_id: "p".into(),
            title: "T".into(),
            description: String::new(),
            author: String::new(),
            artwork_url: None,
            language: None,
            categories: vec![],
            visibility: None,
        }
    );
}

#[test]
fn update_owned_podcast_round_trips() {
    let a = PublishAction::UpdateOwnedPodcast {
        podcast_id: "pod-9".into(),
        title: Some("New Title".into()),
        description: None,
        author: Some("New Author".into()),
        artwork_url: Some("https://new".into()),
        visibility: Some("private".into()),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"update_owned_podcast""#));
    assert!(json.contains(r#""title":"New Title""#));
    assert!(json.contains(r#""author":"New Author""#));
    assert!(json.contains(r#""visibility":"private""#));
    // `None` fields are omitted on the wire (partial update).
    assert!(!json.contains("description"));
    let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}

#[test]
fn delete_owned_podcast_round_trips() {
    let a = PublishAction::DeleteOwnedPodcast {
        podcast_id: "pod-9".into(),
    };
    let json = serde_json::to_string(&a).expect("encode");
    assert!(json.contains(r#""op":"delete_owned_podcast""#));
    let decoded: PublishAction = serde_json::from_str(&json).expect("decode");
    assert_eq!(decoded, a);
}

#[test]
fn execute_emits_dispatch_host_op() {
    let action = PublishAction::CreateOwnedPodcast {
        podcast_id: "pod-1".into(),
    };
    let commands = std::sync::Mutex::new(Vec::<ActorCommand>::new());
    NipF4PublishModule::execute(action, "corr-1", &|cmd| {
        commands.lock().unwrap().push(cmd);
    })
    .expect("execute ok");
    let commands = commands.into_inner().unwrap();
    assert_eq!(commands.len(), 1);
    let ActorCommand::DispatchHostOp { action_json, correlation_id } = &commands[0] else {
        panic!("expected DispatchHostOp");
    };
    assert_eq!(correlation_id, "corr-1");
    let v: serde_json::Value = serde_json::from_str(action_json).expect("json");
    assert_eq!(v["op"], "create_owned_podcast");
    assert_eq!(v["podcast_id"], "pod-1");
}

