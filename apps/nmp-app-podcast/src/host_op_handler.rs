//! Actor-thread handler for podcast/player host operations.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use chrono::Utc;
use nmp_core::substrate::{CapabilityRequest, HostOpHandler};
use nmp_ffi::NmpApp;
use podcast_core::{Episode, PodcastId};
use podcast_feeds::client::{build_feed_request, handle_feed_response, FeedResult};
use podcast_feeds::http::{HttpRequest, HttpResult, HTTP_CAPABILITY_NAMESPACE};

use crate::capability::{
    AudioCommand, DownloadCommand, AUDIO_CAPABILITY_NAMESPACE, DOWNLOAD_CAPABILITY_NAMESPACE,
};
use crate::chapter::handle_fetch_chapters;
use crate::discover_nostr;
use crate::ffi::actions::player_module::PlayerAction;
use crate::ffi::actions::podcast_module::PodcastAction;
use crate::ffi::projections::{NostrShowSummary, PodcastSummary};
use crate::itunes_search::{parse_itunes_results, url_encode};
use crate::player::PlayerActor;
use crate::store::PodcastStore;
use crate::transcript::handle_fetch_transcript;

pub struct PodcastHostOpHandler {
    app: *mut NmpApp,
    store: Arc<Mutex<PodcastStore>>,
    player_actor: Arc<Mutex<PlayerActor>>,
    search_results: Arc<Mutex<Vec<PodcastSummary>>>,
    nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
    rev: Arc<AtomicU64>,
}

unsafe impl Send for PodcastHostOpHandler {}
unsafe impl Sync for PodcastHostOpHandler {}

impl PodcastHostOpHandler {
    pub fn new(
        app: *mut NmpApp,
        store: Arc<Mutex<PodcastStore>>,
        player_actor: Arc<Mutex<PlayerActor>>,
        search_results: Arc<Mutex<Vec<PodcastSummary>>>,
        nostr_results: Arc<Mutex<Vec<NostrShowSummary>>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self { app, store, player_actor, search_results, nostr_results, rev }
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
                let episodes = match self.store.lock() {
                    Ok(s) => {
                        let existing: Vec<Episode> = s.episodes_for(podcast_id).to_vec();
                        merge_episodes(parsed.episodes, existing)
                    }
                    Err(_) => parsed.episodes,
                };
                let etag_out = http_result.header("etag").map(str::to_owned);
                let lm_out = http_result.header("last-modified").map(str::to_owned);
                match self.store.lock() {
                    Ok(mut s) => {
                        s.subscribe(parsed.podcast, episodes);
                        s.update_refresh_metadata(podcast_id, etag_out, lm_out);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        serde_json::json!({"ok": true})
                    }
                    Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
                }
            }
            Ok(FeedResult::NotModified { .. }) => serde_json::json!({"ok": true, "not_modified": true}),
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:?}")}),
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

    fn handle_play(&self, episode_id: String, correlation_id: &str) -> serde_json::Value {
        let (podcast_id, url, position_secs) = {
            match self.store.lock() {
                Ok(s) => match s.episode_playback_info(&episode_id) {
                    Some(info) => info,
                    None => return serde_json::json!({"ok": false, "error": format!("episode not found: {episode_id}")}),
                },
                Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
            }
        };
        {
            if let Ok(mut actor) = self.player_actor.lock() {
                actor.stage_load(&episode_id, Some(podcast_id), &url, position_secs);
            }
        }
        self.rev.fetch_add(1, Ordering::Relaxed);
        let load_cmd = AudioCommand::load(&url, position_secs);
        if let Err(e) = self.dispatch_audio(&load_cmd, correlation_id) {
            return serde_json::json!({"ok": false, "error": e});
        }
        if let Err(e) = self.dispatch_audio(&AudioCommand::Play, correlation_id) {
            return serde_json::json!({"ok": false, "error": e});
        }
        serde_json::json!({"ok": true})
    }

    fn handle_player_action(&self, action: PlayerAction, correlation_id: &str) -> serde_json::Value {
        match action {
            PlayerAction::Play { episode_id } => self.handle_play(episode_id, correlation_id),
            PlayerAction::Pause => {
                match self.dispatch_audio(&AudioCommand::Pause, correlation_id) {
                    Ok(_) => serde_json::json!({"ok": true}),
                    Err(e) => serde_json::json!({"ok": false, "error": e}),
                }
            }
            PlayerAction::Seek { position_secs } => {
                match self.dispatch_audio(&AudioCommand::seek(position_secs), correlation_id) {
                    Ok(_) => serde_json::json!({"ok": true}),
                    Err(e) => serde_json::json!({"ok": false, "error": e}),
                }
            }
            PlayerAction::SetSpeed { speed } => {
                if let Ok(mut a) = self.player_actor.lock() { a.set_speed(speed); }
                self.rev.fetch_add(1, Ordering::Relaxed);
                match self.dispatch_audio(&AudioCommand::SetSpeed { speed }, correlation_id) {
                    Ok(_) => serde_json::json!({"ok": true}),
                    Err(e) => serde_json::json!({"ok": false, "error": e}),
                }
            }
            PlayerAction::SetVolume { volume } => {
                if let Ok(mut a) = self.player_actor.lock() { a.set_volume(volume); }
                self.rev.fetch_add(1, Ordering::Relaxed);
                match self.dispatch_audio(&AudioCommand::SetVolume { volume }, correlation_id) {
                    Ok(_) => serde_json::json!({"ok": true}),
                    Err(e) => serde_json::json!({"ok": false, "error": e}),
                }
            }
            PlayerAction::SetSleepTimer { secs } => {
                if let Ok(mut a) = self.player_actor.lock() {
                    match secs {
                        Some(s) if s > 0 => a.arm_sleep_timer(Duration::from_secs(s), SystemTime::now()),
                        _ => a.cancel_sleep_timer(),
                    }
                }
                self.rev.fetch_add(1, Ordering::Relaxed);
                match self.dispatch_audio(&AudioCommand::SetSleepTimer { secs }, correlation_id) {
                    Ok(_) => serde_json::json!({"ok": true}),
                    Err(e) => serde_json::json!({"ok": false, "error": e}),
                }
            }
            PlayerAction::Stop => {
                match self.dispatch_audio(&AudioCommand::Stop, correlation_id) {
                    Ok(_) => serde_json::json!({"ok": true}),
                    Err(e) => serde_json::json!({"ok": false, "error": e}),
                }
            }
            PlayerAction::Enqueue { episode_id } => self.handle_enqueue(episode_id),
            PlayerAction::Dequeue { episode_id } => self.handle_dequeue(episode_id),
            PlayerAction::ClearQueue => self.handle_clear_queue(),
            PlayerAction::PlayNext => self.handle_play_next(correlation_id),
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
            };
        }
        if let Ok(action) = serde_json::from_str::<PlayerAction>(action_json) {
            return self.handle_player_action(action, correlation_id);
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
