//! `PodcastAction` dispatch (subscribe / refresh / search / download /
//! settings) extracted from `host_op_handler.rs` to keep that file under
//! the 500-LOC hard ceiling (AGENTS.md). The methods stay on
//! `PodcastHostOpHandler` via `impl` blocks in this sibling module and
//! its `podcast_actions_feed` sub-sibling.
//!
//! Lock discipline (inherited from the parent module):
//! * Never hold a `PodcastStore` lock across a capability dispatch.
//! * Notifications + auto-downloads fire AFTER the store lock is released.

use std::sync::atomic::Ordering;

use podcast_feeds::http::{HttpRequest, HttpResult};
use uuid::Uuid;

use crate::chapter::handle_fetch_chapters;
use crate::discover_nostr;
use crate::ffi::actions::podcast_module::PodcastAction;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::transcript::handle_fetch_transcript;

impl PodcastHostOpHandler {
    pub(super) fn handle_podcast_action(
        &self,
        action: PodcastAction,
        correlation_id: &str,
    ) -> serde_json::Value {
        match action {
            PodcastAction::Subscribe { feed_url } => {
                self.handle_subscribe(feed_url, correlation_id)
            }
            PodcastAction::Unsubscribe { podcast_id } => self.handle_unsubscribe(podcast_id),
            PodcastAction::Refresh { podcast_id } => {
                self.handle_refresh(podcast_id, correlation_id)
            }
            PodcastAction::RefreshAll => self.handle_refresh_all(correlation_id),
            PodcastAction::SearchItunes { query } => {
                self.handle_search_itunes(query, correlation_id)
            }
            PodcastAction::ImportOpml { content } => {
                self.handle_import_opml(content, correlation_id)
            }
            PodcastAction::Download { episode_id } => {
                self.handle_download(episode_id, correlation_id)
            }
            PodcastAction::DeleteDownload { episode_id } => {
                self.handle_delete_download(episode_id)
            }
            PodcastAction::FetchTranscript { episode_id } => handle_fetch_transcript(
                &self.store,
                &self.transcripts,
                &self.rev,
                episode_id,
                |req| self.dispatch_http(req, correlation_id),
            ),
            PodcastAction::FetchChapters { episode_id } => {
                handle_fetch_chapters(&self.store, &self.rev, episode_id, |req| {
                    self.dispatch_http(req, correlation_id)
                })
            }
            PodcastAction::DiscoverNostr { query, relay_url } => {
                discover_nostr::handle_discover_nostr(
                    query,
                    relay_url,
                    &self.nostr_results,
                    &self.rev,
                    |req| self.dispatch_nostr_relay(req, correlation_id),
                    |req| self.dispatch_http(req, correlation_id),
                )
            }
            PodcastAction::UpdateSettings { has_completed_onboarding } => {
                self.handle_update_settings(has_completed_onboarding)
            }
            PodcastAction::GenerateBriefing => {
                crate::briefings_handler::handle_generate_briefing(
                    &self.briefing,
                    &self.rev,
                    Some(&self.store),
                    Some(&self.runtime),
                )
            }
            PodcastAction::FetchComments { episode_id } => {
                crate::comments_handler::handle_fetch_comments(
                    self.app,
                    &self.store,
                    &self.comments_cache,
                    &self.rev,
                    &episode_id,
                    correlation_id,
                )
            }
            PodcastAction::PostComment { episode_id, content } => {
                crate::comments_handler::handle_post_comment(
                    self.app,
                    &self.store,
                    &self.identity,
                    &self.comments_cache,
                    &self.rev,
                    &episode_id,
                    &content,
                    correlation_id,
                )
            }
            PodcastAction::SetAutoDownload { podcast_id, enabled, wifi_only } => {
                self.handle_set_auto_download(podcast_id, enabled, wifi_only)
            }
            PodcastAction::DispatchDeferredWifiDownloads => {
                self.handle_dispatch_deferred_wifi_downloads(correlation_id)
            }
            PodcastAction::FetchContacts => crate::social_handler::handle_fetch_contacts(self),
            PodcastAction::PublishAgentNote {
                recipient_pubkey_hex,
                content,
                root_event_id,
            } => crate::agent_note_handler::handle_publish_agent_note(
                self.app,
                &self.identity,
                &recipient_pubkey_hex,
                &content,
                root_event_id.as_deref(),
                correlation_id,
            ),
            PodcastAction::FetchAgentNotes => {
                crate::agent_note_handler::handle_fetch_agent_notes(
                    self.app,
                    &self.identity,
                    &self.agent_notes,
                    &self.rev,
                    correlation_id,
                )
            }
            PodcastAction::StarEpisode { episode_id, starred } => {
                match self.store.lock() {
                    Ok(mut s) => match s.set_episode_starred(&episode_id, starred) {
                        Some(new_value) => {
                            self.rev.fetch_add(1, Ordering::Relaxed);
                            serde_json::json!({"ok": true, "starred": new_value})
                        }
                        None => serde_json::json!({"ok": false, "error": format!("episode not found: {episode_id}")}),
                    },
                    Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
                }
            }
            PodcastAction::SetEpisodeTriage { decisions } => {
                self.handle_set_episode_triage(decisions)
            }
            PodcastAction::MarkEpisodesMetadataIndexed { episode_ids } => {
                self.handle_mark_episodes_metadata_indexed(episode_ids)
            }
            PodcastAction::SetEpisodeTranscriptStatus { episode_id, status, message } => {
                self.handle_set_episode_transcript_status(episode_id, status, message)
            }
        }
    }

    fn handle_set_episode_triage(
        &self,
        decisions: Vec<crate::ffi::actions::podcast_module::EpisodeTriagePatch>,
    ) -> serde_json::Value {
        match self.store.lock() {
            Ok(mut s) => {
                let mut changed = false;
                for patch in decisions {
                    if s.set_episode_triage(
                        patch.episode_id,
                        &patch.decision,
                        patch.is_hero,
                        patch.rationale,
                    ) {
                        changed = true;
                    }
                }
                // Single rev bump for the whole batch — one snapshot tick.
                if changed {
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }

    fn handle_mark_episodes_metadata_indexed(
        &self,
        episode_ids: Vec<String>,
    ) -> serde_json::Value {
        match self.store.lock() {
            Ok(mut s) => {
                let changed = s.mark_episodes_metadata_indexed(episode_ids);
                if changed {
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }

    fn handle_set_episode_transcript_status(
        &self,
        episode_id: String,
        status: String,
        message: Option<String>,
    ) -> serde_json::Value {
        match self.store.lock() {
            Ok(mut s) => {
                let changed = s.set_transcript_status(episode_id, &status, message);
                if changed {
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }

    fn handle_dispatch_deferred_wifi_downloads(&self, correlation_id: &str) -> serde_json::Value {
        let pending = match self.store.lock() {
            Ok(mut s) => s.drain_pending_wifi_downloads(),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        // Revalidate each entry before dispatch. Separate into:
        //   dispatch_now  — current network allows it and show still wants it.
        //   keep_pending  — still valid but not allowed on current network yet
        //                   (network flapped back to cellular; requeue them).
        //   drop          — unsubscribed, download disabled, or already on disk.
        let mut dispatch_now = Vec::new();
        let mut keep_pending = Vec::new();
        match self.store.lock() {
            Ok(s) => {
                let is_on_wifi = s.is_on_wifi();
                for (ep_id, url) in pending {
                    let Some(podcast_id) = s.podcast_id_for_episode(&ep_id) else { continue };
                    if !s.is_auto_download_enabled(podcast_id) { continue }
                    if s.episode_is_downloaded(&ep_id) { continue }
                    let wifi_only = s.wifi_only_for(podcast_id);
                    if wifi_only && !is_on_wifi {
                        // Network flapped back to cellular — requeue rather than drop.
                        keep_pending.push((ep_id, url));
                    } else {
                        dispatch_now.push((ep_id, url));
                    }
                }
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        // Re-add entries that are still valid but need Wi-Fi.
        if !keep_pending.is_empty() {
            if let Ok(mut s) = self.store.lock() {
                s.add_pending_wifi_downloads(keep_pending);
            }
        }
        let count = dispatch_now.len();
        for (episode_id, url) in dispatch_now {
            let cmd = crate::capability::DownloadCommand::start(url, episode_id, None);
            let _ = self.dispatch_download(&cmd, correlation_id);
        }
        serde_json::json!({"ok": true, "dispatched": count})
    }

    fn handle_search_itunes(&self, query: String, correlation_id: &str) -> serde_json::Value {
        let encoded = crate::itunes::url_encode(&query);
        let search_url = format!(
            "https://itunes.apple.com/search?media=podcast&entity=podcast&limit=25&term={encoded}"
        );
        let req = HttpRequest::get(search_url, [("Accept", "application/json")]);
        let http_result = match self.dispatch_http(&req, correlation_id) {
            Ok(r) => r,
            Err(e) => return serde_json::json!({"ok": false, "error": e}),
        };
        let body = match &http_result {
            HttpResult::Ok { body, .. } => body.as_str(),
            HttpResult::Error { message } => {
                return serde_json::json!({"ok": false, "error": message})
            }
        };
        let results = crate::itunes::parse_itunes_results(body);
        match self.search_results.lock() {
            Ok(mut r) => {
                *r = results;
                self.rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "search_results poisoned"}),
        }
    }

    fn handle_download(&self, episode_id_str: String, correlation_id: &str) -> serde_json::Value {
        let url = {
            match self.store.lock() {
                Ok(s) => match s.episode_enclosure_url(&episode_id_str) {
                    Some((_id, url)) => url,
                    None => {
                        return serde_json::json!({
                            "ok": false,
                            "error": format!("episode not found: {episode_id_str}")
                        })
                    }
                },
                Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
            }
        };
        let command = match self.download_queue.lock() {
            Ok(mut q) => q.enqueue(episode_id_str, url),
            Err(_) => return serde_json::json!({"ok": false, "error": "download_queue poisoned"}),
        };
        self.rev.fetch_add(1, Ordering::Relaxed);
        if let Some(cmd) = command {
            if let Err(e) = self.dispatch_download(&cmd, correlation_id) {
                return serde_json::json!({"ok": false, "error": e});
            }
        }
        serde_json::json!({"ok": true})
    }

    fn handle_update_settings(&self, has_completed_onboarding: Option<bool>) -> serde_json::Value {
        let mut mutated = false;
        match self.store.lock() {
            Ok(mut s) => {
                if let Some(value) = has_completed_onboarding {
                    if s.has_completed_onboarding() != value {
                        s.set_onboarding_complete(value);
                        mutated = true;
                    }
                }
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
        if mutated {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
        serde_json::json!({"ok": true})
    }

    fn handle_set_auto_download(
        &self,
        podcast_id_str: String,
        enabled: bool,
        wifi_only: bool,
    ) -> serde_json::Value {
        let uuid = match podcast_id_str.parse::<Uuid>() {
            Ok(u) => u,
            Err(_) => return serde_json::json!({"ok": false, "error": "invalid podcast_id"}),
        };
        let podcast_id = podcast_core::PodcastId::new(uuid);
        match self.store.lock() {
            Ok(mut s) => {
                s.set_auto_download(podcast_id, enabled);
                s.set_wifi_only(podcast_id, wifi_only);
                self.rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
    }

    fn handle_delete_download(&self, episode_id_str: String) -> serde_json::Value {
        let removed_path = {
            match self.store.lock() {
                Ok(mut s) => match s.episode_enclosure_url(&episode_id_str) {
                    Some((ep_id, _url)) => s.clear_local_path(&ep_id),
                    None => None,
                },
                Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
            }
        };
        if let Some(path) = removed_path {
            let _ = std::fs::remove_file(&path);
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
        serde_json::json!({"ok": true})
    }
}
