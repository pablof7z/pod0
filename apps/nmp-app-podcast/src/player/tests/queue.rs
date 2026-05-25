//! Unit tests for the playback queue ("Up Next").
//!
//! The queue lives on [`PlayerActor`] as a `Vec<String>` of episode ids.
//! All mutations go through `enqueue` / `dequeue` / `clear_queue` /
//! `pop_next`. Per D7 the kernel owns ordering; the iOS shell only
//! dispatches actions and renders the projection.

use crate::player::PlayerActor;

#[test]
fn new_actor_has_empty_queue() {
    let actor = PlayerActor::new();
    assert!(actor.queue().is_empty());
}

#[test]
fn enqueue_appends_and_preserves_order() {
    let mut actor = PlayerActor::new();
    actor.enqueue("ep-1");
    actor.enqueue("ep-2");
    actor.enqueue("ep-3");
    assert_eq!(actor.queue(), &["ep-1", "ep-2", "ep-3"]);
}

#[test]
fn enqueue_is_idempotent_by_id() {
    let mut actor = PlayerActor::new();
    actor.enqueue("ep-1");
    actor.enqueue("ep-2");
    actor.enqueue("ep-1"); // duplicate — should be ignored
    assert_eq!(actor.queue(), &["ep-1", "ep-2"]);
}

#[test]
fn dequeue_removes_first_occurrence_only() {
    let mut actor = PlayerActor::new();
    actor.enqueue("ep-1");
    actor.enqueue("ep-2");
    actor.enqueue("ep-3");
    actor.dequeue("ep-2");
    assert_eq!(actor.queue(), &["ep-1", "ep-3"]);
}

#[test]
fn dequeue_missing_id_is_noop() {
    let mut actor = PlayerActor::new();
    actor.enqueue("ep-1");
    actor.dequeue("ep-missing");
    assert_eq!(actor.queue(), &["ep-1"]);
}

#[test]
fn clear_queue_empties_everything() {
    let mut actor = PlayerActor::new();
    actor.enqueue("ep-1");
    actor.enqueue("ep-2");
    actor.clear_queue();
    assert!(actor.queue().is_empty());
}

#[test]
fn pop_next_returns_and_removes_front() {
    let mut actor = PlayerActor::new();
    actor.enqueue("ep-1");
    actor.enqueue("ep-2");
    assert_eq!(actor.pop_next().as_deref(), Some("ep-1"));
    assert_eq!(actor.queue(), &["ep-2"]);
    assert_eq!(actor.pop_next().as_deref(), Some("ep-2"));
    assert!(actor.queue().is_empty());
}

#[test]
fn pop_next_on_empty_returns_none() {
    let mut actor = PlayerActor::new();
    assert!(actor.pop_next().is_none());
}
