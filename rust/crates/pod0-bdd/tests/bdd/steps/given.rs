//! `Given` — the world before the app acts: which podcast feeds exist out
//! there and what bytes they serve. Every step here only STAGES data on
//! [`PodWorld`]; nothing touches disk or opens the facade until a later step
//! calls `ensure_started`.

use cucumber::given;

use crate::world::PodWorld;

#[given(regex = r#"^a podcast "([^"]+)" publishes its feed at "([^"]+)" with episodes? (.+)$"#)]
async fn podcast_publishes_feed(w: &mut PodWorld, title: String, url: String, list: String) {
    let episodes = pod0_bdd::parse_quoted_list(&list);
    assert!(
        !episodes.is_empty(),
        "pod0-bdd: expected at least one quoted episode title in {list:?}"
    );
    w.stage_feed(&title, &url, &episodes);
}

#[given(regex = r#"^the feed at "([^"]+)" serves bytes that are not a podcast feed$"#)]
async fn feed_serves_garbage(w: &mut PodWorld, url: String) {
    w.stage_broken_feed(&url);
}
