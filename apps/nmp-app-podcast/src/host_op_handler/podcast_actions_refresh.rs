//! Feed refresh, OPML import, and refresh side effects.
//!
//! Kept separate from subscribe / ensure-known handling so each host-op module
//! stays under the repository's 500-line hard ceiling.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use chrono::Utc;
use podcast_core::{Episode, EpisodeId, PodcastId};
use podcast_feeds::client::{build_feed_request, handle_feed_response, FeedResult};

use crate::capability::NotificationCommand;
use crate::host_op_handler::PodcastHostOpHandler;
use crate::host_op_handler_helpers::{changed_metadata_ids, merge_episodes};
use crate::picks_handler::refresh_picks_into_slot;
use crate::store::episodes_to_auto_download;

impl PodcastHostOpHandler {
    pub(super) fn handle_refresh(
        &self,
        podcast_id_str: String,
        correlation_id: &str,
    ) -> serde_json::Value {
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
            self.auto_refresh_picks();
        }
        result
    }

    pub(super) fn handle_refresh_all(&self, correlation_id: &str) -> serde_json::Value {
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
            self.auto_refresh_picks();
        }
        if errors.is_empty() {
            serde_json::json!({"ok": true})
        } else {
            serde_json::json!({"ok": true, "partial_errors": errors})
        }
    }

    pub(super) fn handle_import_opml(
        &self,
        content: String,
        correlation_id: &str,
    ) -> serde_json::Value {
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
                let error_msg = result["error"]
                    .as_str()
                    .unwrap_or("unknown error")
                    .to_string();
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

    pub(super) fn refresh_one(
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
                let (
                    episodes,
                    new_for_notification,
                    to_auto_download,
                    podcast_title,
                    stale_triage_ids,
                ) = match self.store.lock() {
                    Ok(mut s) => {
                        let existing: Vec<Episode> = s.episodes_for(podcast_id).to_vec();
                        let existing_guids: HashSet<String> =
                            existing.iter().map(|e| e.guid.clone()).collect();
                        let new_for_notification: Vec<(String, String)> = if existing.is_empty() {
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
                        let wifi_only = s.wifi_only_for(podcast_id);
                        let is_on_wifi = s.is_on_wifi();
                        let (to_auto_download, deferred) = episodes_to_auto_download(
                            &parsed.episodes,
                            &existing_guids,
                            s.local_paths(),
                            auto_on,
                            wifi_only,
                            is_on_wifi,
                        );
                        if !deferred.is_empty() {
                            s.add_pending_wifi_downloads(
                                deferred
                                    .into_iter()
                                    .map(|(id, url)| (id.0.to_string(), url))
                                    .collect(),
                            );
                        }
                        let podcast_title = parsed.podcast.title.clone();
                        // Episodes whose triage-relevant metadata changed on
                        // this refresh have now-stale cached LLM scores.
                        let stale_triage_ids = changed_metadata_ids(&parsed.episodes, &existing);
                        let merged = merge_episodes(parsed.episodes.clone(), existing);
                        (
                            merged,
                            new_for_notification,
                            to_auto_download,
                            podcast_title,
                            stale_triage_ids,
                        )
                    }
                    Err(_) => (
                        parsed.episodes.clone(),
                        Vec::new(),
                        Vec::new(),
                        parsed.podcast.title.clone(),
                        Vec::new(),
                    ),
                };
                let etag_out = http_result.header("etag").map(str::to_owned);
                let lm_out = http_result.header("last-modified").map(str::to_owned);
                let write_ok = match self.store.lock() {
                    Ok(mut s) => {
                        s.upsert_known_podcast(parsed.podcast, episodes);
                        s.update_refresh_metadata(podcast_id, etag_out, lm_out);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(_) => false,
                };
                if !write_ok {
                    return serde_json::json!({"ok": false, "error": "store poisoned"});
                }
                if !stale_triage_ids.is_empty() {
                    if let Ok(mut cache) = self.inbox_triage_cache.lock() {
                        for id in &stale_triage_ids {
                            cache.remove(id);
                        }
                    }
                }
                for (episode_id, episode_title) in new_for_notification {
                    let cmd = NotificationCommand::schedule_new_episode(
                        episode_title,
                        &podcast_title,
                        episode_id,
                    );
                    let _ = self.dispatch_notification(&cmd, correlation_id);
                }
                self.dispatch_auto_downloads(&to_auto_download, correlation_id);
                refresh_picks_into_slot(&self.store, &self.picks, &self.rev);
                serde_json::json!({"ok": true})
            }
            Ok(FeedResult::NotModified { .. }) => {
                serde_json::json!({"ok": true, "not_modified": true})
            }
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:?}")}),
        }
    }

    pub(super) fn dispatch_auto_downloads(
        &self,
        items: &[(EpisodeId, String)],
        correlation_id: &str,
    ) {
        for (episode_id, url) in items {
            // Route through the canonical queue-backed path so fresh-feed
            // auto-downloads share concurrency control, the download-queue
            // snapshot, and the per-episode event log with every other download.
            let _ = self.start_episode_download(&episode_id.0.to_string(), url, correlation_id, true);
        }
    }
}
