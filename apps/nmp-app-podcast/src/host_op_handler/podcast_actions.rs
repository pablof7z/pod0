//! Podcast action concrete handlers for owned shows, search, downloads, and settings.
//!
//! Lock discipline (inherited from the parent module):
//! * Never hold a `PodcastStore` lock across a capability dispatch.
//! * Notifications + auto-downloads fire AFTER the store lock is released.

use std::sync::atomic::Ordering;

use podcast_feeds::http::{HttpRequest, HttpResult};
use uuid::Uuid;

use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_create_podcast(
        &self,
        podcast_id: String,
        title: String,
        description: String,
        author: String,
        feed_url: Option<String>,
        artwork_url: Option<String>,
        language: Option<String>,
        categories: Vec<String>,
        visibility: Option<String>,
        title_is_placeholder: bool,
    ) -> serde_json::Value {
        let visibility = match visibility.as_deref() {
            Some("private") => podcast_core::NostrVisibility::Private,
            _ => podcast_core::NostrVisibility::Public,
        };
        let inserted = match self.store.lock() {
            Ok(mut s) => s.create_podcast(
                &podcast_id,
                title,
                description,
                author,
                feed_url,
                artwork_url,
                language,
                categories,
                visibility,
                title_is_placeholder,
            ),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if !inserted {
            return serde_json::json!({
                "ok": false,
                "error": format!("invalid podcast id: {podcast_id}")
            });
        }
        self.rev.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({"ok": true})
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_add_episode(
        &self,
        podcast_id: String,
        episode_id: String,
        title: String,
        enclosure_url: String,
        description: String,
        duration_secs: Option<f64>,
        image_url: Option<String>,
        chapters: Vec<crate::ffi::actions::podcast_module::EpisodeChapterArg>,
        transcript: Option<String>,
    ) -> serde_json::Value {
        let chapters: Vec<crate::store::owned_ext::EpisodeChapter> = chapters
            .into_iter()
            .map(|c| crate::store::owned_ext::EpisodeChapter {
                start_secs: c.start_secs,
                title: c.title,
                image_url: c.image_url,
                source_episode_id: c.source_episode_id,
            })
            .collect();
        let inserted = match self.store.lock() {
            Ok(mut s) => s.add_episode(
                &podcast_id,
                &episode_id,
                title,
                &enclosure_url,
                description,
                duration_secs,
                image_url,
                chapters,
                transcript,
            ),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if !inserted {
            return serde_json::json!({
                "ok": false,
                "error": format!("could not add episode {episode_id} under podcast {podcast_id}")
            });
        }
        self.rev.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({"ok": true, "episode_id": episode_id})
    }

    pub(super) fn handle_set_episode_triage(
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

    pub(super) fn handle_mark_episodes_metadata_indexed(
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

    pub(super) fn handle_set_episode_transcript_status(
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

    pub(super) fn handle_dispatch_deferred_wifi_downloads(
        &self,
        correlation_id: &str,
    ) -> serde_json::Value {
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
                    let Some(podcast_id) = s.podcast_id_for_episode(&ep_id) else {
                        continue;
                    };
                    if !s.is_auto_download_enabled(podcast_id) {
                        continue;
                    }
                    if s.episode_is_downloaded(&ep_id) {
                        continue;
                    }
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

    pub(super) fn handle_search_itunes(
        &self,
        query: String,
        correlation_id: &str,
    ) -> serde_json::Value {
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

    pub(super) fn handle_download(
        &self,
        episode_id_str: String,
        provided_url: Option<String>,
        correlation_id: &str,
    ) -> serde_json::Value {
        let url = if let Some(url) = provided_url {
            // iOS passed the URL directly — use it without store lookup.
            url
        } else {
            // Fall back to store lookup (for legacy/Rust-side dispatch).
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

    /// Enqueue an on-device model download (kind = `LocalModel`) through the
    /// unified queue. Mirrors [`Self::handle_download`] but always uses the
    /// caller-supplied `url` (models have no episode-store entry) and tags the
    /// item so the executor writes it to the on-device models directory.
    pub(super) fn handle_download_local_model(
        &self,
        model_id: String,
        url: String,
        correlation_id: &str,
    ) -> serde_json::Value {
        let command = match self.download_queue.lock() {
            Ok(mut q) => {
                q.enqueue_with_kind(model_id, url, crate::capability::DownloadKind::LocalModel)
            }
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

    pub(super) fn handle_update_settings(
        &self,
        has_completed_onboarding: Option<bool>,
    ) -> serde_json::Value {
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

    pub(super) fn handle_set_auto_download(
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

    pub(super) fn handle_delete_download(&self, episode_id_str: String) -> serde_json::Value {
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
