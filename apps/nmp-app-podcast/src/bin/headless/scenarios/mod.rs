//! Scenario runner types and top-level `run_all`.

use nmp_app_podcast::PodcastHandle;
use nmp_ffi::NmpApp;

mod agent_chat;
mod agent_notes;
mod comments;
mod discover_nostr;
mod identity;
mod inbox_triage;
mod key_persistence;
mod llm_setup;
mod nipf4_publish;
mod relay_smoke;
mod rss_subscribe;
mod social;
mod wiki;

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
        ("rss_subscribe", rss_subscribe::run(app, handle)),
        ("key_persistence", key_persistence::run(app, handle)),
        ("identity_import", identity::run(app, handle)),
        ("relay_smoke", relay_smoke::run(app, handle)),
        ("wiki_generate", wiki::run(app, handle)),
        ("inbox_triage", inbox_triage::run(app, handle)),
        ("agent_chat", agent_chat::run(app, handle)),
        ("nipf4_publish", nipf4_publish::run(app, handle)),
        ("discover_nostr", discover_nostr::run(app, handle)),
        ("comments", comments::run(app, handle)),
        ("agent_notes", agent_notes::run(app, handle)),
        ("social", social::run(app, handle)),
    ]
}
