/// Unit tests for comments_handler.

#[test]
fn post_comment_rejects_empty_content() {
    use crate::comments_handler::handle_post_comment;
    use crate::store::{identity::IdentityStore, PodcastStore};
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};

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
    use crate::comments_handler::handle_post_comment;
    use crate::store::{identity::IdentityStore, PodcastStore};
    use std::collections::HashMap;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};

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
    use crate::comments_handler::handle_fetch_comments;
    use crate::store::PodcastStore;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};

    let app = std::ptr::null_mut();
    let store = Arc::new(Mutex::new(PodcastStore::new()));

    let viewed = Arc::new(Mutex::new(None::<String>));
    let rev = Arc::new(AtomicU64::new(0));
    let v = handle_fetch_comments(app, &store, &viewed, &rev, None, "no-such-id");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"], "episode not found");
}

#[test]
fn fetch_comments_marks_viewed_episode_and_bumps_rev() {
    use crate::comments_handler::handle_fetch_comments;
    use crate::store::PodcastStore;
    use chrono::Utc;
    use podcast_core::{Episode, EpisodeId, Podcast};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use url::Url;

    let app = std::ptr::null_mut();
    let feed_url = "http://example.com/feed.xml";
    let guid = "comment-guid";
    let mut store = PodcastStore::new();
    let podcast = Podcast::new("Comments Show");
    let podcast_id = podcast.id;
    let episode_id = EpisodeId::from_feed_and_guid(feed_url, guid).0.to_string();
    let episode = Episode::new(
        podcast_id,
        feed_url,
        guid,
        "Comments Episode",
        Url::parse("http://example.com/ep.mp3").unwrap(),
        Utc::now(),
    );
    store.subscribe(podcast, vec![episode]);

    let store = Arc::new(Mutex::new(store));
    let viewed = Arc::new(Mutex::new(None::<String>));
    let rev = Arc::new(AtomicU64::new(0));

    let v = handle_fetch_comments(app, &store, &viewed, &rev, None, &episode_id);

    assert_eq!(v["ok"], true);
    assert_eq!(viewed.lock().unwrap().as_deref(), Some(episode_id.as_str()));
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}
