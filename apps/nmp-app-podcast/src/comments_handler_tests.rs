/// Unit tests for comments_handler.
///
/// The real relay dispatch path is integration-tested in the headless
/// scenario binary (scenarios/comments.rs). The unit tests here cover
/// the validation / short-circuit paths that don't need a live relay.

/// Empty-content guard is enforced before any relay or identity lookup.
#[test]
fn post_comment_rejects_empty_content() {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::AtomicU64;
    use crate::comments_handler::handle_post_comment;
    use crate::store::{identity::IdentityStore, PodcastStore};

    let app = std::ptr::null_mut();
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let cache = Arc::new(Mutex::new(HashMap::new()));
    let rev = Arc::new(AtomicU64::new(0));

    let v = handle_post_comment(app, &store, &identity, &cache, &rev, "ep-1", "", "corr");
    assert_eq!(v["ok"], false);

    let v = handle_post_comment(app, &store, &identity, &cache, &rev, "ep-1", "   ", "corr");
    assert_eq!(v["ok"], false);
}

/// `not signed in` is returned when no identity is loaded.
#[test]
fn post_comment_rejects_when_no_identity() {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::AtomicU64;
    use crate::comments_handler::handle_post_comment;
    use crate::store::{identity::IdentityStore, PodcastStore};

    let app = std::ptr::null_mut();
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let identity = Arc::new(Mutex::new(IdentityStore::new())); // no key
    let cache = Arc::new(Mutex::new(HashMap::new()));
    let rev = Arc::new(AtomicU64::new(0));

    let v = handle_post_comment(
        app, &store, &identity, &cache, &rev, "ep-1", "hello", "corr",
    );
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "not signed in");
}

/// `episode not found` is returned when the store has no matching episode.
#[test]
fn fetch_comments_episode_not_found() {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::AtomicU64;
    use crate::comments_handler::handle_fetch_comments;
    use crate::store::PodcastStore;

    let app = std::ptr::null_mut();
    let store = Arc::new(Mutex::new(PodcastStore::new())); // empty
    let cache = Arc::new(Mutex::new(HashMap::new()));
    let rev = Arc::new(AtomicU64::new(0));

    let v = handle_fetch_comments(app, &store, &cache, &rev, "no-such-id", "corr");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "episode not found");
}
