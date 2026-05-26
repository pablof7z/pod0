//! Scenario runner types and top-level `run_all`.

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

mod key_persistence;
mod relay_smoke;
mod rss_subscribe;

/// Per-scenario outcome.
#[derive(Debug)]
pub enum ScenarioResult {
    Pass,
    /// Scenario was skipped (e.g. required resource not available).
    #[allow(dead_code)]
    Skip(String),
    Fail(String),
}

impl ScenarioResult {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Skip(_) => "SKIP",
            Self::Fail(_) => "FAIL",
        }
    }
}

/// Run all scenarios and return `(name, result)` pairs.
pub fn run_all(
    app: *mut NmpApp,
    handle: *mut PodcastHandle,
) -> Vec<(&'static str, ScenarioResult)> {
    vec![
        (
            "rss_subscribe",
            rss_subscribe::run(app, handle),
        ),
        (
            "key_persistence",
            key_persistence::run(app, handle),
        ),
        ("relay_smoke", relay_smoke::run(app, handle)),
    ]
}
