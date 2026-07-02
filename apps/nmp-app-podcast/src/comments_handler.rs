//! NIP-22 (kind 1111) episode-comments handlers.
//!
//! ## Relay
//!
//! Publish: `nmp.publish { PublishRaw }` — NMP signs with active user signer
//! and routes through its relay pool. No iOS WebSocket, no relay URLs in app.
//!
//! Subscribe: `push_interest_via_nmp` with `kind:1111` + `#i` tag filter and
//! `InterestLifecycle::OneShot`. NMP opens the subscription; events arrive via
//! [`CommentsObserver`] registered at init.
//!
//! ## Wire shape (NIP-22 / NIP-73)
//!
//! kind 1111 events reference the episode via NIP-73 tags:
//! * `["i", "podcast:item:guid:<guid>"]` — the target content identifier
//! * `["k", "podcast:item:guid"]` — the target content kind namespace
//!
//! ## Cache
//!
//! Comments are stored in `PodcastHostOpHandler::comments_cache`
//! (`Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>`) keyed by episode_id.
//! The snapshot builder projects the cache slice for the now-playing episode.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nostr::nips::nip19::ToBech32;

use nmp_planner::interest::{InterestId, InterestLifecycle, InterestScope, LogicalInterest};
use nmp_planner::stable_hash::stable_hash64;
use nmp_core::substrate::{KernelEvent, ViewDependencies};
use nmp_core::ObservedProjectionSink;

use crate::comments_anchor::episode_nip73_anchor;
use crate::ffi::projections::CommentSummary;
use crate::nmp_dispatch::{publish_raw_via_nmp, push_interest_via_nmp};
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::{identity::IdentityStore, PodcastStore};
use nmp_native_runtime::NmpApp;

// ── subscribe helpers ────────────────────────────────────────────────────────

fn comments_interest(anchor: &str) -> LogicalInterest {
    ViewDependencies {
        kinds: vec![1111],
        tag_refs: vec![("i".to_string(), anchor.to_string())],
        limit: Some(100),
        ..Default::default()
    }
    .into_logical_interest(
        InterestId(stable_hash64(&format!("podcast.comments.{anchor}"))),
        InterestScope::Global,
        InterestLifecycle::OneShot,
    )
}

/// Fetch kind-1111 comments for `episode_id` via NMP relay pool.
/// Registers a `OneShot` interest; [`CommentsObserver`] writes results to the
/// cache as events arrive.
pub fn handle_fetch_comments(
    app: *mut NmpApp,
    store: &Arc<Mutex<PodcastStore>>,
    viewed_comments_episode_id: &Arc<Mutex<Option<String>>>,
    rev: &Arc<AtomicU64>,
    snapshot_signal: Option<&SnapshotUpdateSignal>,
    episode_id: &str,
) -> serde_json::Value {
    let anchor = match store.lock() {
        Ok(s) => match episode_nip73_anchor(&s, episode_id) {
            Some(a) => a,
            None => return serde_json::json!({"ok": false, "error": "episode not found"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };
    // Mark this episode as the one whose comments are being viewed so the
    // snapshot projects its cache slice (not just the now-playing episode's).
    let viewed_changed = viewed_comments_episode_id
        .lock()
        .ok()
        .map(|mut viewed| {
            let changed = viewed.as_deref() != Some(episode_id);
            if changed {
                *viewed = Some(episode_id.to_string());
            }
            changed
        })
        .unwrap_or(false);
    push_interest_via_nmp(app, &format!("podcast.comments.{anchor}"), comments_interest(&anchor));
    if viewed_changed {
        if let Some(signal) = snapshot_signal {
            signal.bump();
        } else {
            rev.fetch_add(1, Ordering::Relaxed);
        }
    }
    serde_json::json!({"ok": true, "status": "subscribed", "anchor": anchor})
}

/// Sign and publish a kind-1111 comment for `episode_id`.
/// NMP signs with the active user signer — no secret bytes in app code.
/// Optimistically prepends the new comment to the local cache.
pub fn handle_post_comment(
    app: *mut NmpApp,
    store: &Arc<Mutex<PodcastStore>>,
    identity: &Arc<Mutex<IdentityStore>>,
    comments_cache: &Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>,
    rev: &Arc<AtomicU64>,
    episode_id: &str,
    content: &str,
) -> serde_json::Value {
    if content.trim().is_empty() {
        return serde_json::json!({"ok": false, "error": "empty comment"});
    }
    // Check identity before store lookup so "not signed in" is the primary error.
    let my_npub = match identity.lock() {
        Ok(id) if id.pubkey_hex.is_none() => {
            return serde_json::json!({"ok": false, "error": "not signed in"});
        }
        Ok(id) => id
            .pubkey_hex
            .as_deref()
            .and_then(|h| nostr::PublicKey::parse(h).ok())
            .and_then(|pk| pk.to_bech32().ok())
            .unwrap_or_default(),
        Err(_) => return serde_json::json!({"ok": false, "error": "identity poisoned"}),
    };
    let anchor = match store.lock() {
        Ok(s) => match episode_nip73_anchor(&s, episode_id) {
            Some(a) => a,
            None => return serde_json::json!({"ok": false, "error": "episode not found"}),
        },
        Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
    };

    let tags = vec![
        vec!["i".to_string(), anchor.clone()],
        vec!["k".to_string(), "podcast:item:guid".to_string()],
    ];
    let status = publish_raw_via_nmp(app, 1111, &tags, content);

    // Optimistic cache update.
    let optimistic = CommentSummary {
        id: uuid::Uuid::new_v4().to_string(),
        author_npub: my_npub,
        author_name: None,
        content: content.to_string(),
        created_at: chrono::Utc::now().timestamp(),
    };
    if let Ok(mut cache) = comments_cache.lock() {
        cache
            .entry(episode_id.to_string())
            .or_default()
            .insert(0, optimistic);
    }
    rev.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({"ok": true, "status": status})
}

// ── observer ─────────────────────────────────────────────────────────────────

/// Receives inbound kind:1111 events from NMP's relay pool and writes them
/// into `comments_cache` keyed by episode_id (resolved via store anchor lookup).
pub struct CommentsObserver {
    store: Arc<Mutex<PodcastStore>>,
    comments_cache: Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
}

impl CommentsObserver {
    pub fn new(
        store: Arc<Mutex<PodcastStore>>,
        comments_cache: Arc<Mutex<HashMap<String, Vec<CommentSummary>>>>,
        rev: Arc<AtomicU64>,
    ) -> Self {
        Self {
            store,
            comments_cache,
            rev,
            snapshot_signal: None,
        }
    }

    pub(crate) fn with_snapshot_signal(mut self, snapshot_signal: SnapshotUpdateSignal) -> Self {
        self.snapshot_signal = Some(snapshot_signal);
        self
    }
}

impl ObservedProjectionSink for CommentsObserver {
    fn on_kernel_event(&self, event: &KernelEvent) {
        if event.kind != 1111 {
            return;
        }
        let Some(anchor) = event
            .tags
            .iter()
            .find(|t| t.first().map(|s| s == "i").unwrap_or(false))
            .and_then(|t| t.get(1))
            .cloned()
        else {
            return;
        };

        // Reverse-lookup episode_id from anchor by scanning the store.
        let episode_id = {
            let Ok(store) = self.store.lock() else {
                return;
            };
            store.episode_id_for_anchor(&anchor)
        };
        let Some(episode_id) = episode_id else {
            return;
        };

        let author_npub = nostr::PublicKey::parse(&event.author)
            .ok()
            .and_then(|pk| pk.to_bech32().ok())
            .unwrap_or_else(|| event.author.clone());

        let comment = CommentSummary {
            id: event.id.clone(),
            author_npub,
            author_name: None,
            content: event.content.clone(),
            created_at: event.created_at as i64,
        };

        if let Ok(mut cache) = self.comments_cache.lock() {
            let entry = cache.entry(episode_id).or_default();
            if !entry.iter().any(|c| c.id == comment.id) {
                entry.push(comment);
                entry.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                if let Some(signal) = &self.snapshot_signal {
                    signal.bump();
                } else {
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "comments_handler_tests.rs"]
mod tests;
