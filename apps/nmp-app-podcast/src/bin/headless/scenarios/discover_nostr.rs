//! Scenario: dispatch `podcast.discover_nostr` and verify kind:10154 results
//! arrive from `relay.primal.net`.
//!
//! ## Skip precondition — content probe, not just TCP reachability
//!
//! Discovery can only be asserted when the reference relay actually serves
//! `kind:10154` (NIP-F4 show) events. `kind:10154` is a sparse, niche kind
//! whose availability on any given relay varies over time — a relay that
//! carried shows last week may carry none today. A bare TCP reachability probe
//! (the previous guard) is therefore too weak: on a hosted CI runner
//! `relay.primal.net:443` is reachable but may currently serve zero
//! `kind:10154` events, in which case the pipeline has nothing to surface and
//! the scenario would fail for a reason that has nothing to do with the app.
//!
//! So the guard directly probes `relay.primal.net` for `kind:10154` first:
//!   * no events (or unreachable) → **Skip** — nothing to discover.
//!   * ≥1 event                   → run the assertion. The relay demonstrably
//!     has shows, so if the app's discovery returns empty that is a real
//!     delivery regression and the scenario **Fails** (the content probe
//!     strengthens the precondition; it never weakens the assertion).
//!
//! Uses the real WebSocket relay capability path that PR 9 wired into the
//! `DiscoverNostr` handler.
//!
//! ## Race-safe polling
//!
//! `wait_for` detects rev changes after `last_rev` is sampled. When the
//! actor completes the action (relay 8 s timeout + HTTP fallback) between
//! `dispatch()` returning and `wait_for` reading `last_rev`, the rev bump
//! is invisible to the poller. We therefore also read the snapshot directly
//! after any `wait_for` timeout to catch the "already done" case.

use std::time::Duration;

use nmp_app_podcast::PodcastHandle;
use nmp_native_runtime::NmpApp;

use crate::harness::{dispatch, snapshot, wait_for};
use crate::relay_client;
use crate::scenarios::ScenarioResult;
use crate::scenarios::ScenarioResult::{Fail, Pass, Skip};

const RELAY_URL: &str = "wss://relay.primal.net";

/// Content-level precondition probe: does `relay.primal.net` currently serve
/// any `kind:10154` show event? Returns `true` only if at least one is
/// received before EOSE / timeout. An unreachable relay or an empty result
/// both return `false` (→ Skip).
fn relay_serves_kind_10154() -> bool {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return false;
    };
    let filter = serde_json::json!({ "kinds": [10154], "limit": 1 });
    let events = rt.block_on(relay_client::subscribe_until_eose(
        "headless-discover-probe",
        &filter,
        &[RELAY_URL.to_string()],
        Duration::from_secs(8),
    ));
    events
        .iter()
        .any(|ev| ev.get("kind").and_then(serde_json::Value::as_u64) == Some(10154))
}

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    if !relay_serves_kind_10154() {
        return Skip(format!(
            "{RELAY_URL} unreachable or currently serves no kind:10154 shows — nothing to discover"
        ));
    }

    // Dispatch a no-query discovery sweep — kind:10154 browse on relay.primal.net.
    let result = dispatch(
        app,
        "podcast",
        serde_json::json!({
            "op": "discover_nostr",
            "consumer_id": "headless-discover-nostr",
            "query": null
        }),
    );

    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("dispatch rejected: {err}"));
    }

    // Wait for nostr_results to be non-empty (40 s ceiling: relay uses 8 s
    // timeout, HTTP fallback adds ~5 s, plus actor scheduling jitter).
    //
    // If wait_for times out, do a direct snapshot read: the action may have
    // completed before wait_for sampled last_rev (actor can be that fast when
    // relay returns EOSE quickly and HTTP fallback has low latency).
    let update = match wait_for(handle, 40_000, |u| !u.nostr_results.is_empty()) {
        Ok(u) => u,
        Err(_) => {
            // Race-safe fallback: read the current snapshot directly.
            match snapshot(handle) {
                Some(snap) if !snap.nostr_results.is_empty() => snap,
                _ => {
                    return Fail(
                        "nostr_results still empty after 40 s, yet the content probe found \
                         kind:10154 shows on the relay — discovery pipeline failed to surface them"
                            .into(),
                    );
                }
            }
        }
    };

    let first = &update.nostr_results[0];
    if first.title.is_empty() {
        return Fail(format!(
            "first result has empty title (event_id={})",
            first.event_id
        ));
    }

    Pass
}
