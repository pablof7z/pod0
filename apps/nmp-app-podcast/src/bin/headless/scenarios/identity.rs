//! Scenario: import an nsec and confirm `active_account` surfaces in the
//! snapshot within 5 seconds.

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;
use serde_json::json;

use crate::harness::{dispatch, wait_for};
use crate::fixtures;
use super::ScenarioResult::{self, Fail, Pass};

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // Import the hardcoded test nsec. This should immediately set
    // `active_account` on the snapshot and bump `rev`.
    let res = dispatch(app, "podcast.identity", json!({"type": "ImportNsec", "nsec": fixtures::HEADLESS_TEST_NSEC}));
    // A successful dispatch returns `{"correlation_id":"..."}`.
    // An immediate rejection returns `{"error":"..."}`.
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("ImportNsec dispatch rejected: {err}"));
    }

    // Wait for `active_account` to appear in the snapshot.
    match wait_for(handle, 5_000, |u| u.active_account.is_some()) {
        Ok(u) => {
            let acc = u.active_account.unwrap();
            if !acc.npub.starts_with("npub1") {
                return Fail(format!("npub has wrong prefix: {}", acc.npub));
            }
            if acc.npub != fixtures::HEADLESS_TEST_NPUB {
                return Fail(format!(
                    "npub mismatch: expected {} got {}",
                    fixtures::HEADLESS_TEST_NPUB, acc.npub
                ));
            }
            if acc.mode != "local_key" {
                return Fail(format!("mode wrong: {}", acc.mode));
            }
            Pass
        }
        Err(e) => Fail(e),
    }
}
