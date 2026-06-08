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
use podcast_core::{Podcast, PodcastId};
use podcast_feeds::client::{build_feed_request, handle_feed_response, FeedResult};
use uuid::Uuid;

use crate::feed_fetch::{FeedFetchMode, PendingFeedFetch};
use crate::host_op_handler::PodcastHostOpHandler;
use crate::host_op_handler_helpers::merge_episodes;
use crate::picks_handler::refresh_picks_into_slot;

impl PodcastHostOpHandler {
    /// Subscribe to a feed, optimistically.
    ///
    /// The podcast row is inserted and marked followed **synchronously** (no
    /// network), so it appears on the very next projection tick — subscribe
    /// feels instant. The RSS fetch + episode hydration then runs through the
    /// **async** HTTP capability (off the actor thread); when the platform
    /// reports the body back, [`crate::feed_fetch::FeedFetchCoordinator`] parses
    /// it, merges episodes, and bumps the snapshot rev. See
    /// `docs/plan/optimistic-subscribe-async-http.md`.
    pub(super) fn handle_subscribe(
        &self,
        feed_url: String,
        _correlation_id: &str,
    ) -> serde_json::Value {
        let url = match url::Url::parse(&feed_url) {
            Ok(u) => u,
            Err(e) => return serde_json::json!({"ok": false, "error": format!("bad url: {e}")}),
        };
        // Snapshot any existing row for this feed under a short lock.
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
        let known_row = known.is_some();
        let podcast_id = known
            .as_ref()
            .map(|(id, _, _, _)| *id)
            .unwrap_or_else(PodcastId::generate);

        // Optimistic insert. A previously-known (unsubscribed) feed just gets
        // its follow flag flipped so its cached metadata + episodes survive; a
        // brand-new feed gets a placeholder titled from the feed host that the
        // async hydration replaces with the real parsed metadata.
        let inserted = match self.store.lock() {
            Ok(mut s) => {
                if known_row {
                    s.mark_subscribed(podcast_id)
                } else {
                    let mut placeholder = Podcast::new(placeholder_title(&url));
                    placeholder.id = podcast_id;
                    placeholder.feed_url = Some(url.clone());
                    placeholder.title_is_placeholder = true;
                    s.subscribe(placeholder, Vec::new());
                    true
                }
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        if !inserted {
            return serde_json::json!({"ok": false, "error": "podcast not found"});
        }
        // Surface the optimistic row immediately.
        if let Some(signal) = &self.snapshot_signal {
            signal.bump();
        } else {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }

        // Hydrate episodes in the background. Only a known feed carries cache
        // validators worth a conditional GET; a fresh subscribe sends an
        // unconditional GET so it can't 304 before any body has landed.
        let cache = if known_row {
            known.as_ref().and_then(|(_, etag, last_modified, _)| {
                feed_cache(etag.as_deref(), last_modified.as_deref())
            })
        } else {
            None
        };
        let req = build_feed_request(&url, cache.as_ref());
        let request_id = Uuid::new_v4().to_string();
        self.feed_fetch.register(
            request_id.clone(),
            PendingFeedFetch {
                mode: FeedFetchMode::Subscribe,
                podcast_id,
                url,
                known: known_row,
            },
        );
        self.dispatch_http_async(&request_id, req);

        serde_json::json!({
            "ok": true,
            "status": "subscribing",
            "podcast_id": podcast_id.0.to_string()
        })
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

/// Human-ish placeholder title for the optimistic subscribe row, shown until
/// the feed metadata hydrates. Uses the feed host (sans a leading `www.`) so
/// the row reads as e.g. "example.com" rather than a raw URL.
fn placeholder_title(url: &url::Url) -> String {
    url.host_str()
        .map(|h| h.strip_prefix("www.").unwrap_or(h).to_string())
        .unwrap_or_else(|| url.to_string())
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
