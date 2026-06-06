//! Feed-lifecycle handlers: subscribe / ensure-known / unsubscribe.
//!
//! Extracted from `podcast_actions.rs` to keep that file under the 500-LOC
//! hard ceiling (AGENTS.md). All methods remain on `PodcastHostOpHandler`.
//!
//! Lock discipline (inherited from the parent module):
//! * Never hold a `PodcastStore` lock across a capability dispatch.
//! * Notifications + auto-downloads fire AFTER the store lock is released.

use std::sync::atomic::Ordering;

use chrono::Utc;
use podcast_core::PodcastId;
use podcast_feeds::client::{build_feed_request, handle_feed_response, FeedResult};
use uuid::Uuid;

use crate::host_op_handler::PodcastHostOpHandler;
use crate::host_op_handler_helpers::merge_episodes;
use crate::picks_handler::refresh_picks_into_slot;

impl PodcastHostOpHandler {
    pub(super) fn handle_subscribe(
        &self,
        feed_url: String,
        correlation_id: &str,
    ) -> serde_json::Value {
        let url = match url::Url::parse(&feed_url) {
            Ok(u) => u,
            Err(e) => return serde_json::json!({"ok": false, "error": format!("bad url: {e}")}),
        };
        let known = match self.store.lock() {
            Ok(s) => s.podcast_by_feed_url(&url).map(|p| {
                (
                    p.id,
                    p.etag.clone(),
                    p.last_modified.clone(),
                    s.is_subscribed(p.id),
                )
            }),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if known
            .as_ref()
            .map(|(_, _, _, subscribed)| *subscribed)
            .unwrap_or(false)
        {
            return serde_json::json!({"ok": false, "error": "already subscribed"});
        }
        let podcast_id = known
            .as_ref()
            .map(|(id, _, _, _)| *id)
            .unwrap_or_else(PodcastId::generate);
        let cache = known.as_ref().and_then(|(_, etag, last_modified, _)| {
            feed_cache(etag.as_deref(), last_modified.as_deref())
        });
        let req = podcast_feeds::client::build_feed_request(&url, cache.as_ref());
        let http_result = match self.dispatch_http(&req, correlation_id) {
            Ok(r) => r,
            Err(e) => return serde_json::json!({"ok": false, "error": e}),
        };
        let result = match handle_feed_response(&url, podcast_id, &http_result, None, Utc::now()) {
            Ok(FeedResult::Parsed { parsed, .. }) => {
                let etag_out = http_result.header("etag").map(str::to_owned);
                let lm_out = http_result.header("last-modified").map(str::to_owned);
                // Lock scope ends before `refresh_picks_into_slot` so we never
                // attempt to re-acquire a `Mutex` we already hold (which is
                // non-reentrant on macOS and would deadlock).
                let write_ok = match self.store.lock() {
                    Ok(mut s) => {
                        let episodes = if known.is_some() {
                            let existing = s.episodes_for(podcast_id).to_vec();
                            merge_episodes(parsed.episodes, existing)
                        } else {
                            parsed.episodes
                        };
                        s.subscribe(parsed.podcast, episodes);
                        s.update_refresh_metadata(podcast_id, etag_out, lm_out);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(_) => false,
                };
                if write_ok {
                    refresh_picks_into_slot(&self.store, &self.picks, &self.rev);
                    serde_json::json!({"ok": true})
                } else {
                    serde_json::json!({"ok": false, "error": "store poisoned"})
                }
            }
            Ok(FeedResult::NotModified { .. }) => {
                if known.is_none() {
                    return serde_json::json!({
                        "ok": false,
                        "error": "feed returned not modified before a podcast row existed"
                    });
                }
                let etag_out = http_result.header("etag").map(str::to_owned);
                let lm_out = http_result.header("last-modified").map(str::to_owned);
                let write_ok = match self.store.lock() {
                    Ok(mut s) => {
                        let ok = s.mark_subscribed(podcast_id);
                        if ok {
                            s.update_refresh_metadata(podcast_id, etag_out, lm_out);
                        }
                        if ok {
                            self.rev.fetch_add(1, Ordering::Relaxed);
                        }
                        ok
                    }
                    Err(_) => false,
                };
                if write_ok {
                    serde_json::json!({"ok": true, "not_modified": true, "podcast_id": podcast_id.0.to_string()})
                } else {
                    serde_json::json!({"ok": false, "error": "podcast not found"})
                }
            }
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:?}")}),
        };
        if result["ok"] == true {
            self.auto_categorize();
            self.auto_refresh_picks();
        }
        result
    }

    pub(super) fn handle_ensure_podcast(
        &self,
        feed_url: String,
        correlation_id: &str,
    ) -> serde_json::Value {
        let url = match url::Url::parse(&feed_url) {
            Ok(u) => u,
            Err(e) => return serde_json::json!({"ok": false, "error": format!("bad url: {e}")}),
        };
        let known = match self.store.lock() {
            Ok(s) => s
                .podcast_by_feed_url(&url)
                .map(|p| (p.id, p.etag.clone(), p.last_modified.clone())),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };

        let podcast_id = known
            .as_ref()
            .map(|(id, _, _)| *id)
            .unwrap_or_else(PodcastId::generate);
        let cache = known.as_ref().and_then(|(_, etag, last_modified)| {
            feed_cache(etag.as_deref(), last_modified.as_deref())
        });
        let req = build_feed_request(&url, cache.as_ref());
        let http_result = match self.dispatch_http(&req, correlation_id) {
            Ok(r) => r,
            Err(e) => return serde_json::json!({"ok": false, "error": e}),
        };

        match handle_feed_response(&url, podcast_id, &http_result, None, Utc::now()) {
            Ok(FeedResult::Parsed { parsed, .. }) => {
                let etag_out = http_result.header("etag").map(str::to_owned);
                let lm_out = http_result.header("last-modified").map(str::to_owned);
                let write_ok = match self.store.lock() {
                    Ok(mut s) => {
                        let existing = s.episodes_for(podcast_id).to_vec();
                        let episodes = merge_episodes(parsed.episodes, existing);
                        s.upsert_known_podcast(parsed.podcast, episodes);
                        s.update_refresh_metadata(podcast_id, etag_out, lm_out);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                        true
                    }
                    Err(_) => false,
                };
                if write_ok {
                    serde_json::json!({"ok": true, "podcast_id": podcast_id.0.to_string()})
                } else {
                    serde_json::json!({"ok": false, "error": "store poisoned"})
                }
            }
            Ok(FeedResult::NotModified { .. }) => {
                if known.is_some() {
                    let etag_out = http_result.header("etag").map(str::to_owned);
                    let lm_out = http_result.header("last-modified").map(str::to_owned);
                    if let Ok(mut s) = self.store.lock() {
                        s.update_refresh_metadata(podcast_id, etag_out, lm_out);
                        self.rev.fetch_add(1, Ordering::Relaxed);
                    }
                    serde_json::json!({
                        "ok": true,
                        "not_modified": true,
                        "podcast_id": podcast_id.0.to_string()
                    })
                } else {
                    serde_json::json!({
                        "ok": false,
                        "error": "feed returned not modified before a podcast row existed"
                    })
                }
            }
            Err(e) => serde_json::json!({"ok": false, "error": format!("{e:?}")}),
        }
    }

    pub(super) fn handle_unsubscribe(&self, podcast_id_str: String) -> serde_json::Value {
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
                refresh_picks_into_slot(&self.store, &self.picks, &self.rev);
                serde_json::json!({"ok": true})
            }
            Err(_) => serde_json::json!({"ok": false, "error": "invalid podcast_id"}),
        }
    }
}

fn feed_cache(
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Option<podcast_feeds::refresh::policy::EtagCache> {
    if etag.is_some() || last_modified.is_some() {
        Some(podcast_feeds::refresh::policy::EtagCache::with_headers(
            Utc::now(),
            etag.map(str::to_owned),
            last_modified.map(str::to_owned),
        ))
    } else {
        None
    }
}
