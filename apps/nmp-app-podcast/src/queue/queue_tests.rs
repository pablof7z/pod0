use super::*;
#[test]
fn new_queue_is_empty() {
    let q = PlaybackQueue::new();
    assert!(q.items().is_empty());
}
#[test]
fn add_to_end_pushes_back() {
    let mut q = PlaybackQueue::new();
    q.add_to_end("a");
    q.add_to_end("b");
    q.add_to_end("c");
    assert_eq!(q.items(), &["a".to_owned(), "b".to_owned(), "c".to_owned()]);
}
#[test]
fn add_to_front_pushes_front() {
    let mut q = PlaybackQueue::new();
    q.add_to_front("a");
    q.add_to_front("b");
    q.add_to_front("c");
    // c was added last but to the front, so plays first.
    assert_eq!(q.items(), &["c".to_owned(), "b".to_owned(), "a".to_owned()]);
}
#[test]
fn add_to_end_dedups_by_moving() {
    let mut q = PlaybackQueue::new();
    q.add_to_end("a");
    q.add_to_end("b");
    q.add_to_end("a"); // re-queue "a" at the back
    assert_eq!(q.items(), &["b".to_owned(), "a".to_owned()]);
}
#[test]
fn add_to_front_dedups_by_moving() {
    let mut q = PlaybackQueue::new();
    q.add_to_end("a");
    q.add_to_end("b");
    q.add_to_front("b"); // cut the line — was at back, now at front
    assert_eq!(q.items(), &["b".to_owned(), "a".to_owned()]);
}
#[test]
fn remove_existing_id() {
    let mut q = PlaybackQueue::new();
    q.add_to_end("a");
    q.add_to_end("b");
    q.add_to_end("c");
    q.remove("b");
    assert_eq!(q.items(), &["a".to_owned(), "c".to_owned()]);
}
#[test]
fn remove_missing_id_is_noop() {
    let mut q = PlaybackQueue::new();
    q.add_to_end("a");
    q.remove("z");
    assert_eq!(q.items(), &["a".to_owned()]);
}
#[test]
fn next_pops_front() {
    let mut q = PlaybackQueue::new();
    q.add_to_end("a");
    q.add_to_end("b");
    assert_eq!(q.next().map(|item| item.episode_id), Some("a".to_owned()));
    assert_eq!(q.items(), &["b".to_owned()]);
    assert_eq!(q.next().map(|item| item.episode_id), Some("b".to_owned()));
    assert!(q.items().is_empty());
}
#[test]
fn next_on_empty_returns_none() {
    let mut q = PlaybackQueue::new();
    assert_eq!(q.next(), None);
}
#[test]
fn clear_drops_everything() {
    let mut q = PlaybackQueue::new();
    q.add_to_end("a");
    q.add_to_end("b");
    q.add_to_end("c");
    q.clear();
    assert!(q.items().is_empty());
    // And `next` after `clear` returns None.
    assert_eq!(q.next(), None);
}
#[test]
fn mixed_ops_preserve_ordering() {
    // Realistic scenario: user adds three to queue, decides one is urgent.
    let mut q = PlaybackQueue::new();
    q.add_to_end("ep-1");
    q.add_to_end("ep-2");
    q.add_to_end("ep-3");
    q.add_to_front("ep-3"); // dedup + move to front
    assert_eq!(
        q.items(),
        &["ep-3".to_owned(), "ep-1".to_owned(), "ep-2".to_owned()]
    );
    assert_eq!(q.next().map(|item| item.episode_id), Some("ep-3".to_owned()));
    assert_eq!(q.items(), &["ep-1".to_owned(), "ep-2".to_owned()]);
}

#[test]
fn bounded_segments_preserve_bounds_and_allow_duplicates() {
    let mut q = PlaybackQueue::new();
    q.add_segment_to_end("ep-1", Some(10.0), Some(20.0));
    q.add_segment_to_end("ep-1", Some(30.0), Some(40.0));
    assert_eq!(q.items(), &["ep-1".to_owned(), "ep-1".to_owned()]);
    let first = q.next().expect("first segment");
    assert_eq!(first.episode_id, "ep-1");
    assert_eq!(first.start_secs, Some(10.0));
    assert_eq!(first.end_secs, Some(20.0));
    let second = q.next().expect("second segment");
    assert_eq!(second.start_secs, Some(30.0));
    assert_eq!(second.end_secs, Some(40.0));
}
#[test]
fn default_is_empty() {
    let q = PlaybackQueue::default();
    assert_eq!(q, PlaybackQueue::new());
    assert!(q.items().is_empty());
}
