//! Scenario: subscribe to a real RSS feed and verify the library populates.
//!
//! The `Subscribe` action is synchronous on the actor thread (HTTP → parse →
//! store write → rev bump). After dispatch returns, a single snapshot poll
//! should observe the populated library, but we use `wait_for` with a 30 s
//! ceiling to absorb any actor scheduling jitter.

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

use crate::fixtures::RSS_FEED_URL;
use crate::harness::{dispatch, probe_tcp, wait_for};
use crate::scenarios::ScenarioResult;

/// Namespace for podcast actions (matches `PodcastActionModule::NAMESPACE`).
const PODCAST_NS: &str = "podcast";

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // Network gate: skip if the feed host is unreachable.
    if !probe_tcp("feeds.megaphone.fm", 443) {
        return ScenarioResult::Skip("feed unreachable (TCP probe failed)".into());
    }

    // Dispatch Subscribe. The action JSON uses the snake_case "op" tag.
    let result = dispatch(
        app,
        PODCAST_NS,
        serde_json::json!({"op": "subscribe", "feed_url": RSS_FEED_URL}),
    );

    // A successful dispatch returns `{"correlation_id": "..."}`.
    // An immediate rejection returns `{"error": "..."}`.
    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        return ScenarioResult::Fail(format!("dispatch rejected: {err}"));
    }

    // Wait for the library to contain at least one podcast with episodes.
    let update = match wait_for(handle, 30_000, |u| {
        !u.library.is_empty() && !u.library[0].episodes.is_empty()
    }) {
        Ok(u) => u,
        Err(msg) => return ScenarioResult::Fail(format!("timeout: {msg}")),
    };

    // Assertions
    let podcast = &update.library[0];
    let feed_url = podcast.feed_url.as_deref().unwrap_or("");
    if !feed_url.contains("megaphone.fm") {
        return ScenarioResult::Fail(format!(
            "expected feed_url to contain 'megaphone.fm', got: {feed_url:?}"
        ));
    }

    let episode = &podcast.episodes[0];
    let title_ok = !episode.title.is_empty();
    let duration_ok = episode.duration_secs.unwrap_or(0.0) > 0.0;

    if !title_ok {
        return ScenarioResult::Fail("first episode has empty title".into());
    }
    if !duration_ok {
        return ScenarioResult::Fail("first episode has zero/missing duration".into());
    }

    ScenarioResult::Pass
}
