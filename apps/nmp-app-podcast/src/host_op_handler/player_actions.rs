use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use crate::ad_skip_handler::{handle_set_ad_segments, hydrate_actor_for_play};
use crate::capability::AudioCommand;
use crate::ffi::actions::player_module::PlayerAction;
use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    fn handle_play(&self, episode_id: String, correlation_id: &str) -> serde_json::Value {
        let (podcast_id, url, position_secs) = {
            match self.store.lock() {
                Ok(s) => match s.episode_playback_info(&episode_id) {
                    Some(info) => info,
                    None => {
                        return serde_json::json!({
                            "ok": false,
                            "error": format!("episode not found: {episode_id}")
                        })
                    }
                },
                Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
            }
        };
        if let Ok(mut actor) = self.player_actor.lock() {
            actor.stage_load(&episode_id, Some(podcast_id), &url, position_secs);
        }
        // Push the persisted ad segments + global toggle into the
        // freshly-staged actor so auto-skip can fire on the very first
        // `Playing` report (no extra round-trip via iOS).
        hydrate_actor_for_play(&self.store, &self.player_actor, &episode_id);
        self.rev.fetch_add(1, Ordering::Relaxed);
        if let Err(e) = self.dispatch_audio(&AudioCommand::load(&url, position_secs), correlation_id) {
            return serde_json::json!({"ok": false, "error": e});
        }
        if let Err(e) = self.dispatch_audio(&AudioCommand::Play, correlation_id) {
            return serde_json::json!({"ok": false, "error": e});
        }
        serde_json::json!({"ok": true})
    }

    pub(super) fn handle_player_action(
        &self,
        action: PlayerAction,
        correlation_id: &str,
    ) -> serde_json::Value {
        match action {
            PlayerAction::Play { episode_id } => self.handle_play(episode_id, correlation_id),
            PlayerAction::Pause => self.dispatch_audio_json(AudioCommand::Pause, correlation_id),
            PlayerAction::Seek { position_secs } => {
                self.dispatch_audio_json(AudioCommand::seek(position_secs), correlation_id)
            }
            PlayerAction::SetSpeed { speed } => {
                if let Ok(mut a) = self.player_actor.lock() {
                    a.set_speed(speed);
                }
                self.rev.fetch_add(1, Ordering::Relaxed);
                self.dispatch_audio_json(AudioCommand::SetSpeed { speed }, correlation_id)
            }
            PlayerAction::SetVolume { volume } => {
                if let Ok(mut a) = self.player_actor.lock() {
                    a.set_volume(volume);
                }
                self.rev.fetch_add(1, Ordering::Relaxed);
                self.dispatch_audio_json(AudioCommand::SetVolume { volume }, correlation_id)
            }
            PlayerAction::SetSleepTimer { secs } => {
                if let Ok(mut a) = self.player_actor.lock() {
                    match secs {
                        Some(s) if s > 0 => a.arm_sleep_timer(Duration::from_secs(s), SystemTime::now()),
                        _ => a.cancel_sleep_timer(),
                    }
                }
                self.rev.fetch_add(1, Ordering::Relaxed);
                self.dispatch_audio_json(AudioCommand::SetSleepTimer { secs }, correlation_id)
            }
            PlayerAction::Stop => self.dispatch_audio_json(AudioCommand::Stop, correlation_id),
            PlayerAction::Enqueue { episode_id } => self.handle_enqueue(episode_id),
            PlayerAction::Dequeue { episode_id } => self.handle_dequeue(episode_id),
            PlayerAction::ClearQueue => self.handle_clear_queue(),
            PlayerAction::PlayNext => self.handle_play_next(correlation_id),
            PlayerAction::SetAdSegments { episode_id, segments } => {
                handle_set_ad_segments(&self.store, &self.player_actor, &self.rev, episode_id, segments)
            }
        }
    }

    fn dispatch_audio_json(&self, cmd: AudioCommand, correlation_id: &str) -> serde_json::Value {
        match self.dispatch_audio(&cmd, correlation_id) {
            Ok(_) => serde_json::json!({"ok": true}),
            Err(e) => serde_json::json!({"ok": false, "error": e}),
        }
    }

    fn handle_enqueue(&self, episode_id: String) -> serde_json::Value {
        let exists = match self.store.lock() {
            Ok(s) => s.episode_playback_info(&episode_id).is_some(),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if !exists {
            return serde_json::json!({"ok": false, "error": format!("episode not found: {episode_id}")});
        }
        match self.player_actor.lock() {
            Ok(mut a) => {
                a.enqueue(&episode_id);
                self.rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
        }
    }

    fn handle_dequeue(&self, episode_id: String) -> serde_json::Value {
        match self.player_actor.lock() {
            Ok(mut a) => {
                a.dequeue(&episode_id);
                self.rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
        }
    }

    fn handle_clear_queue(&self) -> serde_json::Value {
        match self.player_actor.lock() {
            Ok(mut a) => {
                a.clear_queue();
                self.rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
        }
    }

    fn handle_play_next(&self, correlation_id: &str) -> serde_json::Value {
        let next_id = match self.player_actor.lock() {
            Ok(mut a) => a.pop_next(),
            Err(_) => return serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
        };
        match next_id {
            Some(id) => self.handle_play(id, correlation_id),
            None => serde_json::json!({"ok": false, "error": "queue is empty"}),
        }
    }
}
