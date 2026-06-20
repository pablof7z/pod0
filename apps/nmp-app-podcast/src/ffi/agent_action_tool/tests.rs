use super::dispatch;

#[test]
fn unfollow_podcast_plan_requires_podcast_id() {
    let req = serde_json::json!({"op": "unfollow_podcast_plan"});
    let result = dispatch(&req);
    assert!(
        result.get("error").is_some(),
        "missing podcast_id must return error, got: {result}"
    );
}

#[test]
fn unfollow_podcast_plan_returns_podcast_id() {
    let req = serde_json::json!({"op": "unfollow_podcast_plan", "podcast_id": "abc-123"});
    let result = dispatch(&req);
    assert_eq!(
        result["podcast_id"], "abc-123",
        "plan must echo back the podcast_id"
    );
}

#[test]
fn unfollow_podcast_result_was_subscribed() {
    let req = serde_json::json!({
        "op": "unfollow_podcast_result",
        "podcast_id": "abc-123",
        "title": "Test Show",
        "was_subscribed": true,
    });
    let result = dispatch(&req);
    assert_eq!(result["success"], true);
    assert_eq!(result["podcast_id"], "abc-123");
    assert_eq!(result["title"], "Test Show");
    assert_eq!(result["episodes_kept"], true);
    assert_eq!(result["was_subscribed"], true);
    let msg = result["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("Unfollowed"),
        "message should confirm unfollow, got: {msg}"
    );
}

#[test]
fn unfollow_podcast_result_not_subscribed() {
    let req = serde_json::json!({
        "op": "unfollow_podcast_result",
        "podcast_id": "abc-123",
        "was_subscribed": false,
    });
    let result = dispatch(&req);
    assert_eq!(result["success"], true);
    assert_eq!(result["episodes_kept"], true);
    assert_eq!(result["was_subscribed"], false);
    let msg = result["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("not followed"),
        "message should note not-followed, got: {msg}"
    );
}
