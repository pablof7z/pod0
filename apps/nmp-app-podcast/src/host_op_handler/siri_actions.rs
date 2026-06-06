//! Handler for `podcast.siri.*` actions.
//!
//! Episode-selection policy lives here in Rust (D0, D7): the iOS
//! `AppIntents` shell only names the intent; the kernel decides which
//! episode is "latest" or "resume".

use crate::ffi::actions::player_module::PlayerAction;
use crate::ffi::actions::siri_module::SiriAction;
use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    /// Dispatch a `podcast.siri.*` action.
    ///
    /// `PlayLatest` selects the most recently published unplayed episode
    /// across the whole library (or within one podcast when `podcast_id`
    /// is supplied) and delegates to `handle_player_action(Play { … })`.
    ///
    /// `Resume` checks whether the player actor already has an episode
    /// staged; if so it replays it, otherwise it falls back to the same
    /// selection as `PlayLatest { podcast_id: None }`.
    pub(super) fn handle_siri_action(
        &self,
        action: SiriAction,
        correlation_id: &str,
    ) -> serde_json::Value {
        match action {
            SiriAction::PlayLatest { podcast_id } => {
                self.siri_play_latest(podcast_id.as_deref(), correlation_id)
            }
            SiriAction::Resume => self.siri_resume(correlation_id),
        }
    }

    /// Select and play the latest unplayed episode. When `podcast_id` is
    /// `Some`, restricts to that podcast; otherwise scans the whole library.
    fn siri_play_latest(
        &self,
        podcast_id: Option<&str>,
        correlation_id: &str,
    ) -> serde_json::Value {
        let episode_id = match self.store.lock() {
            Ok(s) => {
                let candidates = s.subscribed_podcasts();
                candidates
                    .into_iter()
                    .filter(|(pod, eps)| {
                        // Optionally restrict to a specific podcast.
                        podcast_id
                            .map(|id| pod.id.0.to_string() == id)
                            .unwrap_or(true)
                            && eps.iter().any(|e| !e.played)
                    })
                    .flat_map(|(_, eps)| eps.iter())
                    .filter(|e| !e.played)
                    .max_by_key(|e| e.pub_date)
                    .map(|e| e.id.0.to_string())
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };

        match episode_id {
            Some(id) => {
                self.handle_player_action(PlayerAction::Play { episode_id: id }, correlation_id)
            }
            None => serde_json::json!({"ok": false, "error": "no unplayed episodes"}),
        }
    }

    /// Resume the last-staged episode, or fall back to `siri_play_latest`.
    fn siri_resume(&self, correlation_id: &str) -> serde_json::Value {
        let active_id = match self.player_actor.lock() {
            Ok(a) => a.state().episode_id.clone(),
            Err(_) => return serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
        };
        if let Some(id) = active_id {
            return self
                .handle_player_action(PlayerAction::Play { episode_id: id }, correlation_id);
        }
        self.siri_play_latest(None, correlation_id)
    }
}
