//! Scenario: subscribe to a local mock RSS feed and verify the library populates.
//!
//! A minimal 3-episode RSS feed is served locally by `mock_feed::start()`.
//! The `Subscribe` action is synchronous on the actor thread (HTTP → parse →
//! store write → rev bump). After dispatch returns, `wait_for` polls the
//! atomic revision counter until the library contains at least one podcast
//! with episodes, using a 10 s ceiling to absorb any actor scheduling jitter.
//!
//! The scenario skips gracefully when loopback TCP is unavailable in the
//! current execution environment (some sandboxed CI / container setups block
//! 127.0.0.1 outbound connections even though bind succeeds). On standard
//! Linux runners (e.g. GitHub Actions ubuntu-latest) loopback is always
//! available and the scenario runs in full.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_native_runtime::NmpApp;

use crate::harness::{dispatch, wait_for};
use crate::mock_feed;
use crate::scenarios::ScenarioResult;

/// Namespace for podcast actions (matches `PodcastActionModule::NAMESPACE`).
const PODCAST_NS: &str = "podcast";

/// Verify that loopback TCP is usable in this environment.
///
/// Binds a probe `TcpListener` on an OS-assigned port then immediately
/// connects to it. Returns `false` if either the bind or the connect fails,
/// which is the signal to Skip rather than Fail — the test requires a
/// functioning loopback stack but that is not always guaranteed in deeply
/// sandboxed environments.
///
/// This probe does NOT consume the `mock_feed` connection slot because it
/// uses its own separate listener.
fn probe_loopback() -> bool {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return false;
    };
    let port = match listener.local_addr() {
        Ok(a) => a.port(),
        Err(_) => return false,
    };
    let addr: SocketAddr = match format!("127.0.0.1:{port}").parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_ok()
}

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // Gate on loopback availability before starting the mock server.
    // On ubuntu-latest GitHub Actions runners loopback always works; this
    // guard exists for sandboxed environments where bind succeeds but
    // outbound loopback connections are blocked.
    if !probe_loopback() {
        return ScenarioResult::Skip("loopback TCP unavailable".into());
    }

    // Start a local mock RSS server; no network required.
    let port = mock_feed::start();
    let feed_url = format!("http://127.0.0.1:{port}/feed.xml");

    // Dispatch Subscribe. The action JSON uses the snake_case "op" tag.
    let result = dispatch(
        app,
        PODCAST_NS,
        serde_json::json!({"op": "subscribe", "feed_url": feed_url}),
    );

    // A successful dispatch returns `{"correlation_id": "..."}`.
    // An immediate rejection returns `{"error": "..."}`.
    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        return ScenarioResult::Fail(format!("dispatch rejected: {err}"));
    }

    // Wait for the library to contain at least one podcast with episodes.
    let update = match wait_for(handle, 10_000, |u| {
        !u.library.is_empty() && !u.library[0].episodes.is_empty()
    }) {
        Ok(u) => u,
        Err(msg) => return ScenarioResult::Fail(format!("timeout: {msg}")),
    };

    // Assertions
    let podcast = &update.library[0];
    let stored_feed_url = podcast.feed_url.as_deref().unwrap_or("");
    if !stored_feed_url.contains("127.0.0.1") {
        return ScenarioResult::Fail(format!(
            "expected feed_url to contain '127.0.0.1', got: {stored_feed_url:?}"
        ));
    }

    let episode = &podcast.episodes[0];
    if episode.title.is_empty() {
        return ScenarioResult::Fail("first episode has empty title".into());
    }
    if episode.duration_secs.unwrap_or(0.0) <= 0.0 {
        return ScenarioResult::Fail("first episode has zero/missing duration".into());
    }

    ScenarioResult::Pass
}
