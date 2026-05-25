//! `PodcastAction` dispatch (subscribe / refresh / search / download /
//! settings) extracted from `host_op_handler.rs` to keep that file under
//! the 500-LOC hard ceiling (AGENTS.md). The methods stay on
//! `PodcastHostOpHandler` via an `impl` block in this sibling module.
//!
//! Lock discipline (inherited from the parent module):
//! * Never hold a `PodcastStore` lock across a capability dispatch.
//! * Notifications + auto-downloads fire AFTER the store lock is released.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use chrono::Utc;
use podcast_core::{Episode, EpisodeId, PodcastId};
use podcast_feeds::client::{build_feed_request, handle_feed_response, FeedResult};
use podcast_feeds::http::{HttpRequest, HttpResult};
use uuid::Uuid;

use crate::capability::{DownloadCommand, NotificationCommand};
use crate::chapter::handle_fetch_chapters;
use crate::discover_nostr;
use crate::ffi::actions::podcast_module::PodcastAction;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::host_op_handler_helpers::merge_episodes;
use crate::picks_handler::refresh_picks_into_slot;
use crate::store::episodes_to_auto_download;
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
                    |req| self.dispatch_http(req, correlation_id),
                )
            }
            PodcastAction::UpdateSettings { has_completed_onboarding } => {
                self.handle_update_settings(has_completed_onboarding)
            }
            PodcastAction::GenerateBriefing => {
                crate::briefings_handler::handle_generate_briefing(&self.briefing, &self.rev)
            }
            PodcastAction::FetchComments { episode_id } => {
                crate::comments_handler::handle_fetch_comments(&episode_id)
            }
            PodcastAction::PostComment { episode_id, content } => {
                crate::comments_handler::handle_post_comment(&episode_id, &content)
            }
            PodcastAction::SetAutoDownload { podcast_id, enabled } => {
                self.handle_set_auto_download(podcast_id, enabled)
            }
            PodcastAction::FetchContacts => crate::social_handler::handle_fetch_contacts(),
            PodcastAction::StarEpisode { episode_id, starred } => {
                match self.store.lock() {
                    Ok(mut s) => match s.set_episode_starred(&episode_id, starred) {
                        Some(new_value) => {
                            self.rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            serde_json::json!({"ok": true, "starred": new_value})
                        }
                        None => serde_json::json!({"ok": false, "error": format!("episode not found: {episode_id}")}),
                    },
                    Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
                }
            }
        }
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
        let result = match handle_feed_response(&url, podcast_id, &http_result, None, Utc::now()) {
            Ok(FeedResult::Parsed { parsed, .. }) => match self.store.lock() {
                Ok(mut s) => {
                    s.subscribe(parsed.podcast, parsed.episodes);
                    self.rev.fetch_add(1, Ordering::Relaxed);
                    refresh_picks_into_slot(&self.store, &self.picks, &self.rev);
                    serde_json::json!({"ok": true})
                }
                Err(_) => serde_json::json!({"ok": false, "error": "store poisoned"}),
            },
            Ok(FeedResult::NotModified { .. }) => {
                serde_json::json!({"ok": true, "not_modified": true})
            }
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:?}")}),
        };
        if result["ok"] == true {
            self.auto_categorize();
        }
        result
    }

    fn handle_unsubscribe(&self, podcast_id_str: String) -> serde_json::Value {
        match podcast_id_str.parse::<Uuid>() {
            Ok(uuid) => {
                let id = PodcastId::new(uuid);
                let ok = match self.store.lock() {
                    Ok(mut s) => {
                        s.unsubscribe(id);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(_) => false,
                };
                if !ok {
                    return serde_json::json!({"ok": false, "error": "store poisoned"});
                }
                // Picks may reference episodes from the removed show; recompute
                // so the Home rail doesn't surface dangling rows.
                refresh_picks_into_slot(&self.store, &self.picks, &self.rev);
                serde_json::json!({"ok": true})
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
        let result = self.refresh_one(
            podcast_id,
            &url,
            etag.as_deref(),
            last_modified.as_deref(),
            correlation_id,
        );
        if result["ok"] == true {
            self.auto_categorize();
        }
        result
    }

    fn handle_refresh_all(&self, correlation_id: &str) -> serde_json::Value {
        let infos = match self.store.lock() {
            Ok(s) => s.all_feed_infos(),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        let mut errors = Vec::new();
        let mut any_succeeded = false;
        for (id, url, etag, last_modified) in infos {
            let result = self.refresh_one(
                id,
                &url,
                etag.as_deref(),
                last_modified.as_deref(),
                correlation_id,
            );
            if result["ok"] == true {
                any_succeeded = true;
            } else if let Some(e) = result["error"].as_str() {
                errors.push(format!("{}: {}", url, e));
            }
        }
        // Bump rev so the next snapshot tick recomputes the inbox projection
        // from the freshly-pulled episodes even when every feed returned 304.
        self.rev.fetch_add(1, Ordering::Relaxed);
        if any_succeeded {
            self.auto_categorize();
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
                // Single lock window: snapshot existing list, compute the
                // notification set + auto-download set, then merge forward.
                // Both diffs run BEFORE the subsequent `subscribe` write so a
                // concurrent unsubscribe can't race a stale dispatch through.
                let (episodes, new_for_notification, to_auto_download, podcast_title) =
                    match self.store.lock() {
                        Ok(s) => {
                            let existing: Vec<Episode> = s.episodes_for(podcast_id).to_vec();
                            let existing_guids: HashSet<String> =
                                existing.iter().map(|e| e.guid.clone()).collect();
                            // Only notify on refreshes that follow at least one
                            // prior episode load (i.e. `existing` is non-empty).
                            let new_for_notification: Vec<(String, String)> = if existing.is_empty()
                            {
                                Vec::new()
                            } else {
                                let existing_ids: HashSet<String> =
                                    existing.iter().map(|e| e.id.0.to_string()).collect();
                                parsed
                                    .episodes
                                    .iter()
                                    .filter(|ep| !existing_ids.contains(&ep.id.0.to_string()))
                                    .map(|ep| (ep.id.0.to_string(), ep.title.clone()))
                                    .collect()
                            };
                            let auto_on = s.is_auto_download_enabled(podcast_id);
                            let to_auto_download = episodes_to_auto_download(
                                &parsed.episodes,
                                &existing_guids,
                                s.local_paths(),
                                auto_on,
                            );
                            let podcast_title = parsed.podcast.title.clone();
                            let merged = merge_episodes(parsed.episodes.clone(), existing);
                            (merged, new_for_notification, to_auto_download, podcast_title)
                        }
                        Err(_) => (
                            parsed.episodes.clone(),
                            Vec::new(),
                            Vec::new(),
                            parsed.podcast.title.clone(),
                        ),
                    };
                let etag_out = http_result.header("etag").map(str::to_owned);
                let lm_out = http_result.header("last-modified").map(str::to_owned);
                // Second lock window: commit the merged episodes + refresh
                // metadata. Kept narrow so the dispatches below run with no
                // lock held.
                let write_ok = match self.store.lock() {
                    Ok(mut s) => {
                        s.subscribe(parsed.podcast, episodes);
                        s.update_refresh_metadata(podcast_id, etag_out, lm_out);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(_) => false,
                };
                if !write_ok {
                    return serde_json::json!({"ok": false, "error": "store poisoned"});
                }
                // Lock released - safe to dispatch notifications + downloads.
                for (episode_id, episode_title) in new_for_notification {
                    let cmd = NotificationCommand::schedule_new_episode(
                        episode_title,
                        &podcast_title,
                        episode_id,
                    );
                    let _ = self.dispatch_notification(&cmd, correlation_id);
                }
                self.dispatch_auto_downloads(&to_auto_download, correlation_id);
                // Auto-recompute picks: the library just changed so the
                // pick slot is stale. Takes the store lock independently.
                refresh_picks_into_slot(&self.store, &self.picks, &self.rev);
                serde_json::json!({"ok": true})
            }
            Ok(FeedResult::NotModified { .. }) => {
                serde_json::json!({"ok": true, "not_modified": true})
            }
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:?}")}),
        }
    }

    /// Dispatch one `DownloadCommand::StartDownload` per item, swallowing
    /// per-item failures so a single bad URL doesn't drop the rest of the
    /// batch.
    fn dispatch_auto_downloads(&self, items: &[(EpisodeId, String)], correlation_id: &str) {
        for (episode_id, url) in items {
            let cmd = DownloadCommand::start(url.clone(), episode_id.0.to_string(), None);
            let _ = self.dispatch_download(&cmd, correlation_id);
        }
    }

    fn handle_import_opml(&self, content: String, correlation_id: &str) -> serde_json::Value {
        let parsed = match podcast_feeds::import_opml(&content) {
            Ok(p) => p,
            Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
        };
        let existing_feed_urls: HashSet<String> = match self.store.lock() {
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
                let error_msg =
                    result["error"].as_str().unwrap_or("unknown error").to_string();
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
        let cmd = DownloadCommand::start(url, episode_id_str, None);
        if let Err(e) = self.dispatch_download(&cmd, correlation_id) {
            return serde_json::json!({"ok": false, "error": e});
        }
        serde_json::json!({"ok": true})
    }

    fn handle_update_settings(&self, has_completed_onboarding: Option<bool>) -> serde_json::Value {
        // The empty patch (every field `None`) is a no-op - still returns
        // `{"ok": true}` so the Swift dispatch path doesn't need a branch
        // for "patch with no fields".
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
    }

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
