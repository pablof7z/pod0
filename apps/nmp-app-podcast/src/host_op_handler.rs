//! Actor-thread handler for podcast/player host operations.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use nmp_core::substrate::{CapabilityRequest, HostOpHandler};
use nmp_ffi::NmpApp;
use podcast_core::{Episode, EpisodeId, PodcastId};
use uuid::Uuid;
use podcast_feeds::client::{build_feed_request, handle_feed_response, FeedResult};
use podcast_feeds::http::{HttpRequest, HttpResult, HTTP_CAPABILITY_NAMESPACE};

use crate::capability::{
    notification_command_json, AudioCommand, DownloadCommand, NotificationCommand,
    AUDIO_CAPABILITY_NAMESPACE, DOWNLOAD_CAPABILITY_NAMESPACE, NOTIFICATION_CAPABILITY_NAMESPACE,
};
use crate::chapter::handle_fetch_chapters;
use crate::discover_nostr;
use crate::ffi::actions::player_module::PlayerAction;
use crate::ffi::actions::podcast_module::PodcastAction;
use crate::ffi::actions::queue_module::QueueAction;
use crate::ffi::projections::{BriefingSnapshot, NostrShowSummary, PodcastSummary};
use crate::host_op_handler_helpers::merge_episodes;
use crate::host_op_handler_queue::handle_queue_action;
use crate::itunes_search::{parse_itunes_results, url_encode};
use crate::player::PlayerActor;
use crate::queue::PlaybackQueue;
use crate::store::{episodes_to_auto_download, PodcastStore};
use crate::transcript::handle_fetch_transcript;

mod player_actions;

pub struct PodcastHostOpHandler {
    app: *mut NmpApp,
    store: Arc<Mutex<PodcastStore>>,
    player_actor: Arc<Mutex<PlayerActor>>,
    search_results: Arc<Mutex<Vec<PodcastSummary>>>,
    nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
    briefing: Arc<Mutex<Option<BriefingSnapshot>>>,
    queue: Arc<Mutex<PlaybackQueue>>,
    rev: Arc<AtomicU64>,
}

unsafe impl Send for PodcastHostOpHandler {}
unsafe impl Sync for PodcastHostOpHandler {}

impl PodcastHostOpHandler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app: *mut NmpApp,
        store: Arc<Mutex<PodcastStore>>,
        player_actor: Arc<Mutex<PlayerActor>>,
        search_results: Arc<Mutex<Vec<PodcastSummary>>>,
        nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
        briefing: Arc<Mutex<Option<BriefingSnapshot>>>,
        queue: Arc<Mutex<PlaybackQueue>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self { app, store, player_actor, search_results, nostr_results, briefing, queue, rev }
    }
    fn dispatch_http(&self, req: &HttpRequest, correlation_id: &str) -> Result<HttpResult, String> {
        let payload_json = serde_json::to_string(req).map_err(|e| e.to_string())?;
        let cap_req = CapabilityRequest {
            namespace: HTTP_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let envelope = unsafe { &*self.app }.dispatch_capability(&cap_req);
        serde_json::from_str::<HttpResult>(&envelope.result_json)
            .map_err(|e| format!("decode http result: {e}"))
    }
    fn handle_subscribe(&self, feed_url: String, correlation_id: &str) -> serde_json::Value {
        let url = match url::Url::parse(&feed_url) {
            Ok(u) => u,
            Err(e) => return serde_json::json!({"ok": false, "error": format!("bad url: {e}")}),
        };
        let req = build_feed_request(&url, None);
        let http_result = match self.dispatch_http(&req, correlation_id) {
            Ok(r) => r,
            Err(e) => return serde_json::json!({"ok": false, "error": e}),
        };
        let podcast_id = PodcastId::generate();
        match handle_feed_response(&url, podcast_id, &http_result, None, Utc::now()) {
            Ok(FeedResult::Parsed { parsed, .. }) => match self.store.lock() {
                Ok(mut s) => {
                    s.subscribe(parsed.podcast, parsed.episodes);
                    self.rev.fetch_add(1, Ordering::Relaxed);
                    serde_json::json!({"ok": true})
                }
                Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
            },
            Ok(FeedResult::NotModified { .. }) => serde_json::json!({"ok": true, "not_modified": true}),
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:?}")}),
        }
    }
    fn handle_unsubscribe(&self, podcast_id_str: String) -> serde_json::Value {
        match podcast_id_str.parse::<uuid::Uuid>() {
            Ok(uuid) => {
                let id = PodcastId::new(uuid);
                match self.store.lock() {
                    Ok(mut s) => {
                        s.unsubscribe(id);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        serde_json::json!({"ok": true})
                    }
                    Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
                }
            }
            Err(_) => serde_json::json!({"ok": false, "error": "invalid podcast_id"}),
        }
    }
    fn handle_refresh(&self, podcast_id_str: String, correlation_id: &str) -> serde_json::Value {
        let (podcast_id, url, etag, last_modified) = {
            match self.store.lock() {
                Ok(s) => match s.podcast_by_id_str(&podcast_id_str) {
                    Some(p) => match p.feed_url.clone() {
                        Some(u) => (p.id, u, p.etag.clone(), p.last_modified.clone()),
                        None => return serde_json::json!({"ok": false, "error": "no feed url"}),
                    },
                    None => return serde_json::json!({"ok": false, "error": "podcast not found"}),
                },
                Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
            }
        };
        self.refresh_one(podcast_id, &url, etag.as_deref(), last_modified.as_deref(), correlation_id)
    }
    fn handle_refresh_all(&self, correlation_id: &str) -> serde_json::Value {
        let infos = match self.store.lock() {
            Ok(s) => s.all_feed_infos(),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        let mut errors = Vec::new();
        for (id, url, etag, last_modified) in infos {
            let result = self.refresh_one(id, &url, etag.as_deref(), last_modified.as_deref(), correlation_id);
            if result["ok"] != true {
                if let Some(e) = result["error"].as_str() {
                    errors.push(format!("{}: {}", url, e));
                }
            }
        }
        if errors.is_empty() {
            serde_json::json!({"ok": true})
        } else {
            serde_json::json!({"ok": true, "partial_errors": errors})
        }
    }

    fn refresh_one(
        &self,
        podcast_id: PodcastId,
        url: &url::Url,
        etag: Option<&str>,
        last_modified: Option<&str>,
        correlation_id: &str,
    ) -> serde_json::Value {
        use podcast_feeds::refresh::policy::EtagCache;
        let cache = if etag.is_some() || last_modified.is_some() {
            Some(EtagCache::with_headers(
                Utc::now(),
                etag.map(str::to_owned),
                last_modified.map(str::to_owned),
            ))
        } else {
            None
        };
        let req = build_feed_request(url, cache.as_ref());
        let http_result = match self.dispatch_http(&req, correlation_id) {
            Ok(r) => r,
            Err(e) => return serde_json::json!({"ok": false, "error": e}),
        };
        match handle_feed_response(url, podcast_id, &http_result, None, Utc::now()) {
            Ok(FeedResult::Parsed { parsed, .. }) => {
                // Compute the set of newly-discovered episodes BEFORE the
                // subsequent `subscribe` call writes the parsed list into the
                // store. After `subscribe` lands, the "new" ids would all be
                // present and the diff would be empty. D0: this is a Rust
                // policy decision — the iOS capability never inspects which
                // episode is new, it just schedules whatever Rust hands it.
                //
                // Edge case: when `existing` is empty (first refresh after a
                // wiped store, or a podcast freshly seeded from somewhere
                // other than `handle_subscribe`), every parsed episode looks
                // new. Acceptable v1; revisit if it becomes noisy.
                let (episodes, new_for_notification, podcast_title) = match self.store.lock() {
                    Ok(s) => {
                        let existing: Vec<Episode> = s.episodes_for(podcast_id).to_vec();
                        let existing_ids: std::collections::HashSet<String> =
                            existing.iter().map(|e| e.id.0.to_string()).collect();
                        // Only notify on refreshes that follow at least one
                        // prior episode load. `subscribe` already wrote the
                        // initial enclosure list during the first-subscribe
                        // path, so a non-empty `existing` is the
                        // "we've-seen-this-feed-before" gate.
                        let new_for_notification: Vec<(String, String)> = if existing.is_empty() {
                            Vec::new()
                        } else {
                            parsed
                                .episodes
                                .iter()
                                .filter(|ep| !existing_ids.contains(&ep.id.0.to_string()))
                                .map(|ep| (ep.id.0.to_string(), ep.title.clone()))
                                .collect()
                        };
                        let podcast_title = parsed.podcast.title.clone();
                        let merged = merge_episodes(parsed.episodes.clone(), existing);
                        (merged, new_for_notification, podcast_title)
                    }
                    Err(_) => (parsed.episodes.clone(), Vec::new(), parsed.podcast.title.clone()),
                };
                let etag_out = http_result.header("etag").map(str::to_owned);
                let lm_out = http_result.header("last-modified").map(str::to_owned);
                let subscribe_outcome = match self.store.lock() {
                // Single lock window: snapshot existing guids + auto-download
                // flag + local-paths map, then merge position data forward.
                // We compute the set of new episodes to auto-queue *before*
                // releasing the lock so a concurrent unsubscribe can't race
                // a stale dispatch through.
                let (episodes, to_auto_download) = match self.store.lock() {
                    Ok(s) => {
                        let existing: Vec<Episode> = s.episodes_for(podcast_id).to_vec();
                        let existing_guids: HashSet<String> =
                            existing.iter().map(|e| e.guid.clone()).collect();
                        let auto_on = s.is_auto_download_enabled(podcast_id);
                        let new_eps = episodes_to_auto_download(
                            &parsed.episodes,
                            &existing_guids,
                            s.local_paths(),
                            auto_on,
                        );
                        let merged = merge_episodes(parsed.episodes, existing);
                        (merged, new_eps)
                    }
                    Err(_) => (parsed.episodes, Vec::new()),
                };
                let etag_out = http_result.header("etag").map(str::to_owned);
                let lm_out = http_result.header("last-modified").map(str::to_owned);
                // Second lock window: commit the merged episode list + refresh
                // metadata. Kept narrow so the auto-download dispatches below
                // never run with the store locked (lock discipline at the top
                // of this file).
                match self.store.lock() {
                    Ok(mut s) => {
                        s.subscribe(parsed.podcast, episodes);
                        s.update_refresh_metadata(podcast_id, etag_out, lm_out);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
                };
                // Dispatch notifications AFTER all store locks are released
                // (lock discipline documented at the top of this file). One
                // command per new episode — batching/dedup is a Rust-side
                // policy decision we defer for now.
                if subscribe_outcome["ok"] == true {
                    for (episode_id, episode_title) in new_for_notification {
                        let cmd = NotificationCommand::schedule_new_episode(
                            episode_title,
                            &podcast_title,
                            episode_id,
                        );
                        let _ = self.dispatch_notification(&cmd, correlation_id);
                    }
                }
                subscribe_outcome
                    Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
                }
                // Lock released — safe to dispatch download commands. D7:
                // the iOS capability owns the actual fetch; the kernel only
                // tells it what to start.
                self.dispatch_auto_downloads(&to_auto_download, correlation_id);
                serde_json::json!({"ok": true})
            }
            Ok(FeedResult::NotModified { .. }) => serde_json::json!({"ok": true, "not_modified": true}),
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:?}")}),
        }
    }

    /// Dispatch one `DownloadCommand::StartDownload` per item, swallowing
    /// per-item failures so a single bad URL doesn't drop the rest of the
    /// batch. Used by `refresh_one` after auto-download policy returns a
    /// list of fresh episodes to queue.
    fn dispatch_auto_downloads(
        &self,
        items: &[(EpisodeId, String)],
        correlation_id: &str,
    ) {
        for (episode_id, url) in items {
            let cmd = DownloadCommand::start(
                url.clone(),
                episode_id.0.to_string(),
                None,
            );
            // D6 — errors degrade silently; the next refresh will retry.
            let _ = self.dispatch_download(&cmd, correlation_id);
        }
    }

    fn handle_import_opml(&self, content: String, correlation_id: &str) -> serde_json::Value {
        let parsed = match podcast_feeds::import_opml(&content) {
            Ok(p) => p,
            Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
        };

        let existing_feed_urls: std::collections::HashSet<String> =
            match self.store.lock() {
                Ok(s) => s
                    .all_feed_infos()
                    .into_iter()
                    .map(|(_, url, _, _)| url.to_string())
                    .collect(),
                Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
            };

        let mut imported: usize = 0;
        let mut skipped: usize = 0;
        let mut errors: Vec<serde_json::Value> = Vec::new();

        for podcast in parsed.iter() {
            let Some(feed_url) = podcast.feed_url.as_ref() else {
                continue;
            };
            let feed_url_str = feed_url.to_string();
            if existing_feed_urls.contains(&feed_url_str) {
                skipped += 1;
                continue;
            }
            let result = self.handle_subscribe(feed_url_str.clone(), correlation_id);
            if result["ok"] == true {
                imported += 1;
            } else {
                let error_msg = result["error"].as_str().unwrap_or("unknown error").to_string();
                errors.push(serde_json::json!({
                    "feed_url": feed_url_str,
                    "title": podcast.title.clone(),
                    "error": error_msg,
                }));
            }
        }

        serde_json::json!({
            "ok": true,
            "imported": imported,
            "skipped": skipped,
            "errors": errors,
            "total": parsed.len(),
        })
    }

    fn handle_search_itunes(&self, query: String, correlation_id: &str) -> serde_json::Value {
        let encoded = url_encode(&query);
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
        let results = parse_itunes_results(body);
        match self.search_results.lock() {
            Ok(mut r) => {
                *r = results;
                self.rev.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "search_results poisoned"}),
        }
    }

    fn dispatch_audio(&self, cmd: &AudioCommand, correlation_id: &str) -> Result<(), String> {
        let payload_json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        let req = CapabilityRequest {
            namespace: AUDIO_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }

    fn dispatch_download(&self, cmd: &DownloadCommand, correlation_id: &str) -> Result<(), String> {
        let payload_json = serde_json::to_string(cmd).map_err(|e| e.to_string())?;
        let req = CapabilityRequest {
            namespace: DOWNLOAD_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }

    /// Dispatch a notification command to the iOS executor.
    ///
    /// Fire-and-forget — the notification capability has no back-channel
    /// reports. The capability envelope is constructed exactly like the
    /// audio/download dispatchers above so the iOS-side router can fan out
    /// by namespace without special-casing.
    fn dispatch_notification(
        &self,
        cmd: &NotificationCommand,
        correlation_id: &str,
    ) -> Result<(), String> {
        let payload_json = notification_command_json(cmd);
        let req = CapabilityRequest {
            namespace: NOTIFICATION_CAPABILITY_NAMESPACE.to_owned(),
            correlation_id: correlation_id.to_owned(),
            payload_json,
        };
        let _ = unsafe { &*self.app }.dispatch_capability(&req);
        Ok(())
    }

    fn handle_download(&self, episode_id_str: String, correlation_id: &str) -> serde_json::Value {
        let url = {
            match self.store.lock() {
                Ok(s) => match s.episode_enclosure_url(&episode_id_str) {
                    Some((_id, url)) => url,
                    None => return serde_json::json!({
                        "ok": false,
                        "error": format!("episode not found: {episode_id_str}")
                    }),
                },
                Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
            }
        };
        let cmd = DownloadCommand::start(url, episode_id_str, None);
        if let Err(e) = self.dispatch_download(&cmd, correlation_id) {
            return serde_json::json!({"ok": false, "error": e});
        }
        serde_json::json!({"ok": true})
    }

    fn handle_update_settings(&self, has_completed_onboarding: Option<bool>) -> serde_json::Value {
        // The empty patch (every field `None`) is a no-op — still returns
        // `{"ok": true}` so the Swift dispatch path doesn't need a branch
        // for "patch with no fields."
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
            // Bump rev so iOS re-polls and sees the new `settings` projection.
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
        serde_json::json!({"ok": true})
    fn handle_set_auto_download(
        &self,
        podcast_id_str: String,
        enabled: bool,
    ) -> serde_json::Value {
        let uuid = match podcast_id_str.parse::<Uuid>() {
            Ok(u) => u,
            Err(_) => return serde_json::json!({"ok": false, "error": "invalid podcast_id"}),
        };
        let podcast_id = PodcastId::new(uuid);
        match self.store.lock() {
            Ok(mut s) => {
                s.set_auto_download(podcast_id, enabled);
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

impl HostOpHandler for PodcastHostOpHandler {
    fn handle(&self, action_json: &str, correlation_id: &str) -> serde_json::Value {
        if let Ok(action) = serde_json::from_str::<PodcastAction>(action_json) {
            return match action {
                PodcastAction::Subscribe { feed_url } => self.handle_subscribe(feed_url, correlation_id),
                PodcastAction::Unsubscribe { podcast_id } => self.handle_unsubscribe(podcast_id),
                PodcastAction::Refresh { podcast_id } => self.handle_refresh(podcast_id, correlation_id),
                PodcastAction::RefreshAll => self.handle_refresh_all(correlation_id),
                PodcastAction::SearchItunes { query } => self.handle_search_itunes(query, correlation_id),
                PodcastAction::ImportOpml { content } => self.handle_import_opml(content, correlation_id),
                PodcastAction::Download { episode_id } => self.handle_download(episode_id, correlation_id),
                PodcastAction::DeleteDownload { episode_id } => self.handle_delete_download(episode_id),
                PodcastAction::FetchTranscript { episode_id } => handle_fetch_transcript(&self.store, &self.rev, episode_id, |req| self.dispatch_http(req, correlation_id)),
                PodcastAction::FetchChapters { episode_id } => handle_fetch_chapters(&self.store, &self.rev, episode_id, |req| self.dispatch_http(req, correlation_id)),
                PodcastAction::DiscoverNostr { query, relay_url } => discover_nostr::handle_discover_nostr(query, relay_url, &self.nostr_results, &self.rev, |req| self.dispatch_http(req, correlation_id)),
                PodcastAction::UpdateSettings { has_completed_onboarding } => {
                    self.handle_update_settings(has_completed_onboarding)
                }
                PodcastAction::GenerateBriefing => crate::briefings_handler::handle_generate_briefing(&self.briefing, &self.rev),
                PodcastAction::FetchComments { episode_id } => crate::comments_handler::handle_fetch_comments(&episode_id),
                PodcastAction::PostComment { episode_id, content } => crate::comments_handler::handle_post_comment(&episode_id, &content),
                PodcastAction::SetAutoDownload { podcast_id, enabled } => {
                    self.handle_set_auto_download(podcast_id, enabled)
                }
                PodcastAction::FetchContacts => crate::social_handler::handle_fetch_contacts(),
            };
        }
        if let Ok(action) = serde_json::from_str::<PlayerAction>(action_json) {
            return self.handle_player_action(action, correlation_id);
        }
        if let Ok(action) = serde_json::from_str::<QueueAction>(action_json) {
            return handle_queue_action(&self.queue, &self.rev, action);
        }
        serde_json::json!({"ok": false, "error": format!("unknown action: {action_json}")})
    }
}

fn merge_episodes(fresh: Vec<Episode>, existing: Vec<Episode>) -> Vec<Episode> {
    fresh
        .into_iter()
        .map(|mut ep| {
            if let Some(prev) = existing.iter().find(|e| e.id == ep.id) {
                ep.position_secs = prev.position_secs;
            }
            ep
        })
        .collect()
}
