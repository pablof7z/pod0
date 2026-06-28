use super::*;
use serde_json::{json, Value};

const COORD: &str = "31933:abc:podcast";

fn event(id: &str, pubkey: &str, kind: u32, created_at: i64, tags: Value, content: &str) -> Value {
    json!({
        "id": id,
        "pubkey": pubkey,
        "created_at": created_at,
        "kind": kind,
        "tags": tags,
        "content": content,
        "sig": "",
    })
}

#[test]
fn root_with_reply_and_metadata_resolves() {
    let events = vec![
        event(
            "root1",
            "alice",
            1,
            100,
            json!([["a", COORD], ["t", "feature-request"]]),
            "please add X",
        ),
        event(
            "reply1",
            "bob",
            1,
            150,
            json!([["e", "root1", "", "root"], ["a", COORD]]),
            "+1",
        ),
        event(
            "meta1",
            "maintainer",
            513,
            200,
            json!([["e", "root1"], ["title", "Add X"], ["status", "open"]]),
            "",
        ),
    ];
    let threads = reduce_feedback_threads(&events, COORD);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].event_id, "root1");
    assert_eq!(threads[0].category, "feature-request");
    assert_eq!(threads[0].title.as_deref(), Some("Add X"));
    assert_eq!(threads[0].replies[0].event_id, "reply1");
}

#[test]
fn newest_metadata_wins() {
    let events = vec![
        event("root1", "a", 1, 100, json!([["a", COORD]]), "root"),
        event(
            "old",
            "m",
            513,
            150,
            json!([["e", "root1"], ["status", "open"]]),
            "",
        ),
        event(
            "new",
            "m",
            513,
            200,
            json!([["e", "root1"], ["status", "resolved"]]),
            "",
        ),
    ];
    let threads = reduce_feedback_threads(&events, COORD);
    assert_eq!(threads[0].status_label.as_deref(), Some("resolved"));
}

#[test]
fn roots_without_project_coordinate_are_excluded() {
    let events = vec![
        event("mine", "a", 1, 100, json!([["a", COORD]]), "in project"),
        event(
            "other",
            "a",
            1,
            100,
            json!([["a", "31933:xyz:other"]]),
            "different project",
        ),
    ];
    let threads = reduce_feedback_threads(&events, COORD);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].event_id, "mine");
}

#[test]
fn roots_sorted_newest_first_replies_oldest_first() {
    let events = vec![
        event("r_old", "a", 1, 100, json!([["a", COORD]]), "old root"),
        event("r_new", "a", 1, 300, json!([["a", COORD]]), "new root"),
        event(
            "rep_b",
            "b",
            1,
            250,
            json!([["e", "r_new", "", "root"]]),
            "second",
        ),
        event(
            "rep_a",
            "b",
            1,
            200,
            json!([["e", "r_new", "", "root"]]),
            "first",
        ),
    ];
    let threads = reduce_feedback_threads(&events, COORD);
    assert_eq!(threads[0].event_id, "r_new");
    assert_eq!(threads[1].event_id, "r_old");
    let reply_ids = threads[0]
        .replies
        .iter()
        .map(|reply| reply.event_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(reply_ids, vec!["rep_a", "rep_b"]);
}

#[test]
fn category_defaults_and_aliases() {
    let category_for = |tags: Value| {
        reduce_feedback_threads(&[event("r", "a", 1, 1, tags, "c")], COORD)[0]
            .category
            .clone()
    };
    assert_eq!(category_for(json!([["a", COORD]])), "bug");
    assert_eq!(
        category_for(json!([["a", COORD], ["t", "Praise"]])),
        "praise"
    );
    assert_eq!(
        category_for(json!([["a", COORD], ["category", "question"]])),
        "question"
    );
    assert_eq!(
        category_for(json!([["a", COORD], ["t", "feature request"]])),
        "feature-request"
    );
}

#[test]
fn metadata_falls_back_to_content_json() {
    let events = vec![
        event("root1", "a", 1, 100, json!([["a", COORD]]), "root"),
        event(
            "meta1",
            "m",
            513,
            150,
            json!([["e", "root1"]]),
            r#"{"title":"From JSON","summary":"sum","status_label":"closed"}"#,
        ),
    ];
    let thread = &reduce_feedback_threads(&events, COORD)[0];
    assert_eq!(thread.title.as_deref(), Some("From JSON"));
    assert_eq!(thread.summary.as_deref(), Some("sum"));
    assert_eq!(thread.status_label.as_deref(), Some("closed"));
}
