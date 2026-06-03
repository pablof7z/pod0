/// Unit tests for comments_handler.

#[test]
fn post_comment_rejects_empty_content() {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};
    use crate::comments_handler::handle_post_comment;
    use crate::store::{identity::IdentityStore, PodcastStore};

    let app = std::ptr::null_mut();
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let identity = Arc::new(Mutex::new(IdentityStore::new()));
    let cache = Arc::new(Mutex::new(HashMap::new()));
    let rev = Arc::new(AtomicU64::new(0));

    let v = handle_post_comment(app, &store, &identity, &cache, &rev, "ep-1", "");
    assert_eq!(v["ok"], false);

    let v = handle_post_comment(app, &store, &identity, &cache, &rev, "ep-1", "   ");
    assert_eq!(v["ok"], false);
}

#[test]
fn post_comment_rejects_when_no_identity() {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};
    use crate::comments_handler::handle_post_comment;
    use crate::store::{identity::IdentityStore, PodcastStore};

    let app = std::ptr::null_mut();
    let store = Arc::new(Mutex::new(PodcastStore::new()));
    let identity = Arc::new(Mutex::new(IdentityStore::new())); // no key
    let cache = Arc::new(Mutex::new(HashMap::new()));
    let rev = Arc::new(AtomicU64::new(0));

    let v = handle_post_comment(app, &store, &identity, &cache, &rev, "ep-1", "hello");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "not signed in");
}

#[test]
fn fetch_comments_episode_not_found() {
    use std::sync::{Arc, Mutex};
    use crate::comments_handler::handle_fetch_comments;
    use crate::store::PodcastStore;

    let app = std::ptr::null_mut();
    let store = Arc::new(Mutex::new(PodcastStore::new()));

    let v = handle_fetch_comments(app, &store, "no-such-id");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "episode not found");
}
