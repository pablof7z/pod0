use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use crate::ad_skip_handler::{handle_set_ad_segments, hydrate_actor_for_play};
use crate::capability::{AudioCommand, DownloadCommand};
use crate::ffi::actions::player_module::PlayerAction;
use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    fn handle_play(&self, episode_id: String, correlation_id: &str) -> serde_json::Value {
        let (podcast_id, url, position_secs, needs_download) = {
            match self.store.lock() {
                Ok(s) => match s.episode_playback_info(&episode_id) {
                    Some((pod_id, ep_url, pos)) => {
                        let downloaded = s.episode_is_downloaded(&episode_id);
                        (pod_id, ep_url, pos, !downloaded)
                    }
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
        if let Err(e) = self.dispatch_audio(
            &AudioCommand::load_with_id(&url, position_secs, &episode_id),
            correlation_id,
        ) {
            return serde_json::json!({"ok": false, "error": e});
        }
        if let Err(e) = self.dispatch_audio(&AudioCommand::Play, correlation_id) {
            return serde_json::json!({"ok": false, "error": e});
        }
        // Enqueue a background download for episodes played from remote URL.
        // `DownloadQueue::enqueue` is idempotent: returns `None` (no dispatch)
        // when the item is already queued or in a non-terminal state.
        if needs_download {
            let dl_id = episode_id.clone();
            let dl_url = url.clone();
            self.handle_download_command(|q| q.enqueue(dl_id, dl_url), correlation_id);
        }
        serde_json::json!({"ok": true})
    }

    fn handle_load(&self, episode_id: String, correlation_id: &str) -> serde_json::Value {
        let (podcast_id, url, position_secs, needs_download) = {
            match self.store.lock() {
                Ok(s) => match s.episode_playback_info(&episode_id) {
                    Some((pod_id, ep_url, pos)) => {
                        let downloaded = s.episode_is_downloaded(&episode_id);
                        (pod_id, ep_url, pos, !downloaded)
                    }
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
        hydrate_actor_for_play(&self.store, &self.player_actor, &episode_id);
        self.rev.fetch_add(1, Ordering::Relaxed);
        // Dispatch Load only — no Play. iOS calls Resume when the user taps play.
        let dispatch = self.dispatch_audio(
            &AudioCommand::load_with_id(&url, position_secs, &episode_id),
            correlation_id,
        );
        if let Err(e) = dispatch {
            return serde_json::json!({"ok": false, "error": e});
        }
        // Enqueue a background download for streamed episodes. The UI's play
        // path dispatches `load` (not `play`), and restored mini-player plays
        // skip the Swift-side enqueue, so owning the download-on-play rule here
        // keeps it consistent across every play entry point. Idempotent.
        if needs_download {
            let dl_id = episode_id.clone();
            let dl_url = url.clone();
            self.handle_download_command(|q| q.enqueue(dl_id, dl_url), correlation_id);
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
            PlayerAction::Load { episode_id } => self.handle_load(episode_id, correlation_id),
            PlayerAction::Resume => self.dispatch_audio_json(AudioCommand::Play, correlation_id),
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
            PlayerAction::SkipForward { secs } => self.handle_skip(secs, correlation_id),
            PlayerAction::SkipBackward { secs } => self.handle_skip(-secs, correlation_id),
            PlayerAction::Download { episode_id, url } => {
                self.handle_player_download(episode_id, url, correlation_id)
            }
            PlayerAction::CancelDownload { episode_id } => {
                self.handle_download_command(|q| q.cancel(&episode_id), correlation_id)
            }
            PlayerAction::PauseDownload { episode_id } => {
                self.handle_download_command(|q| q.pause(&episode_id), correlation_id)
            }
            PlayerAction::ResumeDownload { episode_id } => {
                self.handle_download_command(|q| q.resume(&episode_id), correlation_id)
            }
            PlayerAction::CancelAllDownloads => {
                self.handle_download_command(|q| q.cancel_all(), correlation_id)
            }
            PlayerAction::ResetProgress { episode_id } => match self.store.lock() {
                Ok(mut s) => {
                    s.reset_episode_progress(&episode_id);
                    self.rev.fetch_add(1, Ordering::Relaxed);
                    serde_json::json!({"ok": true})
                }
                Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
            },
            PlayerAction::Advance => self.handle_play_next(correlation_id),
            PlayerAction::PersistPosition { episode_id, position_secs } => {
                match self.store.lock() {
                    Ok(mut s) => {
                        s.set_episode_position(&episode_id, position_secs);
                        s.flush_positions();
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        serde_json::json!({"ok": true})
                    }
                    Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
                }
            }
        }
    }

    fn dispatch_audio_json(&self, cmd: AudioCommand, correlation_id: &str) -> serde_json::Value {
        match self.dispatch_audio(&cmd, correlation_id) {
            Ok(_) => serde_json::json!({"ok": true}),
            Err(e) => serde_json::json!({"ok": false, "error": e}),
        }
    }

    fn dispatch_download_json(
        &self,
        cmd: DownloadCommand,
        correlation_id: &str,
    ) -> serde_json::Value {
        match self.dispatch_download(&cmd, correlation_id) {
            Ok(_) => serde_json::json!({"ok": true}),
            Err(e) => serde_json::json!({"ok": false, "error": e}),
        }
    }

    /// `podcast.player.enqueue` — alias for `podcast.queue.add_last`. Appends
    /// to the back of the **canonical** [`PlaybackQueue`] (`self.queue`), the
    /// same queue the snapshot's `Up Next` projection renders from. Validates
    /// the episode exists, then mutates + persists via the shared queue helper.
    fn handle_enqueue(&self, episode_id: String) -> serde_json::Value {
        let exists = match self.store.lock() {
            Ok(s) => s.episode_playback_info(&episode_id).is_some(),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if !exists {
            return serde_json::json!({"ok": false, "error": format!("episode not found: {episode_id}")});
        }
        self.mutate_queue(|q| q.add_to_end(&episode_id))
    }

    /// `podcast.player.dequeue` — alias for `podcast.queue.remove`. Removes the
    /// id from anywhere in the canonical queue (silent no-op when absent).
    fn handle_dequeue(&self, episode_id: String) -> serde_json::Value {
        self.mutate_queue(|q| q.remove(&episode_id))
    }

    /// `podcast.player.clear_queue` — alias for `podcast.queue.clear`. Empties
    /// the canonical queue.
    fn handle_clear_queue(&self) -> serde_json::Value {
        self.mutate_queue(|q| q.clear())
    }

    /// Pop the front of the **canonical** queue and play it. Backs both the
    /// explicit `PlayNext` user action and the `Advance` op. Skips stale heads
    /// (ids whose episode is no longer resolvable in the store) so a removed
    /// episode at the front never strands the valid entries behind it — the
    /// same loop `maybe_auto_advance` runs, minus the `auto_play_next` gate
    /// (this is an explicit user action). Queue and store locks are taken
    /// separately per iteration (never nested) to avoid lock-order hazards.
    fn handle_play_next(&self, correlation_id: &str) -> serde_json::Value {
        loop {
            let popped = match self.queue.lock() {
                Ok(mut q) => q.next(),
                Err(_) => return serde_json::json!({"ok": false, "error": "queue poisoned"}),
            };
            let Some(id) = popped else {
                self.persist_queue();
                self.rev.fetch_add(1, Ordering::Relaxed);
                return serde_json::json!({"ok": false, "error": "queue is empty"});
            };
            let resolvable = match self.store.lock() {
                Ok(s) => s.episode_playback_info(&id).is_some(),
                Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
            };
            if resolvable {
                // Persist the new (popped) queue ordering before handing off to
                // `handle_play`, which dispatches Load+Play and bumps `rev`.
                self.persist_queue();
                return self.handle_play(id, correlation_id);
            }
            // Stale head already popped; continue to the next entry.
        }
    }

    /// Apply a mutation to the canonical [`PlaybackQueue`], persist the new
    /// ordering to `podcasts.json`, and bump `rev` so the next snapshot tick
    /// surfaces it. Mirrors `host_op_handler_queue::handle_queue_action` so the
    /// `podcast.player` queue ops stay byte-identical to `podcast.queue`.
    fn mutate_queue(
        &self,
        f: impl FnOnce(&mut crate::queue::PlaybackQueue),
    ) -> serde_json::Value {
        let items = match self.queue.lock() {
            Ok(mut q) => {
                f(&mut q);
                q.items().to_vec()
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "queue poisoned"}),
        };
        self.rev.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut s) = self.store.lock() {
            s.persist_with_queue(&items);
        }
        serde_json::json!({"ok": true})
    }

    /// Flush the current canonical queue ordering to `podcasts.json` without
    /// otherwise mutating it. Used after `handle_play_next` pops the head.
    fn persist_queue(&self) {
        let items = match self.queue.lock() {
            Ok(q) => q.items().to_vec(),
            Err(_) => return,
        };
        if let Ok(mut s) = self.store.lock() {
            s.persist_with_queue(&items);
        }
    }

    /// Relative seek by `delta_secs` (positive = forward, negative = backward).
    /// Reads the current position from the live actor state so the shell never
    /// needs to track position client-side (D0).
    fn handle_skip(&self, delta_secs: f64, correlation_id: &str) -> serde_json::Value {
        let current = match self.player_actor.lock() {
            Ok(a) => a.state().position_secs,
            Err(_) => return serde_json::json!({"ok": false, "error": "player_actor poisoned"}),
        };
        let target = (current + delta_secs).max(0.0);
        self.dispatch_audio_json(AudioCommand::seek(target), correlation_id)
    }

    fn handle_player_download(
        &self,
        episode_id: String,
        url: String,
        correlation_id: &str,
    ) -> serde_json::Value {
        let exists = match self.store.lock() {
            Ok(s) => s.episode_enclosure_url(&episode_id).is_some(),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if !exists {
            return serde_json::json!({
                "ok": false,
                "error": format!("episode not found: {episode_id}")
            });
        }

        // Route through the one canonical download path so this initiator shares
        // concurrency control, the download-queue snapshot, and the per-episode
        // event log with the `podcast.download` op — otherwise a download started
        // here would emit no pipeline events and the Diagnostics log would stay
        // empty for it.
        match self.start_episode_download(&episode_id, &url, correlation_id, false) {
            Ok(()) => serde_json::json!({"ok": true}),
            Err(e) => serde_json::json!({"ok": false, "error": e}),
        }
    }

    fn handle_download_command(
        &self,
        f: impl FnOnce(&mut crate::download::DownloadQueue) -> Option<DownloadCommand>,
        correlation_id: &str,
    ) -> serde_json::Value {
        let command = match self.download_queue.lock() {
            Ok(mut q) => f(&mut q),
            Err(_) => return serde_json::json!({"ok": false, "error": "download_queue poisoned"}),
        };
        self.rev.fetch_add(1, Ordering::Relaxed);
        match command {
            Some(cmd) => self.dispatch_download_json(cmd, correlation_id),
            None => serde_json::json!({"ok": true}),
        }
    }
}

#[cfg(test)]
#[path = "player_actions_tests.rs"]
mod tests;
