//! Async feed-fetch coordination for the optimistic subscribe path.
//!
//! Subscribing must feel instant. `handle_subscribe`
//! ([`crate::host_op_handler::PodcastHostOpHandler`]) inserts the podcast row
//! optimistically and returns immediately; the actual RSS fetch runs through
//! the **async** HTTP capability ([`podcast_feeds::http::HttpCommand`]) so it
//! never blocks the NMP actor thread. When the platform reports the result
//! back via the HTTP-report FFI ([`crate::ffi::http_report`]), the kernel
//! parses the feed, merges episodes, and bumps the snapshot rev so the
//! freshly-hydrated episodes reach the shell.
//!
//! ## Thread discipline
//!
//! [`FeedFetchCoordinator::register`] runs on the actor thread (from
//! `handle_subscribe`). [`FeedFetchCoordinator::apply_report`] runs on the
//! **platform transport thread** (the iOS `URLSession` completion / Android
//! callback that fires the report FFI), exactly like the download-report
//! channel. It therefore touches only the shared `Arc<Mutex<…>>` state and the
//! snapshot signal — **never** `*mut NmpApp` (which is actor-thread-only). All
//! re-projection rides [`crate::snapshot_signal::SnapshotUpdateSignal::bump`],
//! which wakes the actor's update sink from any thread.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use podcast_core::{Episode, PodcastId};
use podcast_feeds::client::{handle_feed_response, FeedResult};
use podcast_feeds::http::{HttpReport, HttpResult};
use tokio::runtime::Runtime;
use url::Url;

use crate::categorization::handle_run_with_signal as categorization_run_with_signal;
use crate::ffi::projections::AgentPickSummary;
use crate::host_op_handler_helpers::merge_episodes;
use crate::picks_handler::handle_refresh_with_signal as picks_handle_refresh_with_signal;
use crate::snapshot_signal::SnapshotUpdateSignal;
use crate::store::PodcastStore;

/// What to do with a feed body once its async HTTP report arrives. Currently
/// only the subscribe path is async; refresh / ensure / OPML / iTunes keep the
/// synchronous HTTP capability for now (see `docs/plan/optimistic-subscribe-async-http.md`).
#[derive(Clone, Debug)]
pub(crate) enum FeedFetchMode {
    /// A user-initiated subscribe. The optimistic row already exists and is
    /// followed; this fills in real metadata + episodes.
    Subscribe,
}

/// A feed fetch awaiting its async HTTP report, keyed by `request_id` in the
/// coordinator's pending map.
#[derive(Clone, Debug)]
pub(crate) struct PendingFeedFetch {
    pub(crate) mode: FeedFetchMode,
    pub(crate) podcast_id: PodcastId,
    pub(crate) url: Url,
    /// `true` when the podcast row already existed (and may carry cached
    /// episodes) before this fetch — drives the merge-vs-replace decision so a
    /// re-subscribe to a previously-known feed doesn't drop its cached episodes.
    pub(crate) known: bool,
}

/// Owns in-flight async feed fetches plus the shared state needed to apply
/// their results off the actor thread.
///
/// Held as an `Arc` by both [`crate::host_op_handler::PodcastHostOpHandler`]
/// (registers a pending fetch + dispatches the command on the actor thread) and
/// [`crate::ffi::handle::PodcastHandle`] (whose HTTP-report FFI applies the
/// result from the platform transport thread). The two share the same `store`
/// / `rev` / `snapshot_signal` `Arc`s the rest of the kernel uses.
pub(crate) struct FeedFetchCoordinator {
    pending: Mutex<HashMap<String, PendingFeedFetch>>,
    store: Arc<Mutex<PodcastStore>>,
    rev: Arc<AtomicU64>,
    snapshot_signal: Option<SnapshotUpdateSignal>,
    categories: Arc<Mutex<HashMap<String, Vec<String>>>>,
    categorization_in_progress: Arc<AtomicBool>,
    picks: Arc<Mutex<Vec<AgentPickSummary>>>,
    picks_score_in_progress: Arc<AtomicBool>,
    runtime: Arc<Runtime>,
}

impl FeedFetchCoordinator {
    /// Build a coordinator over the kernel's shared state. The two
    /// re-entrancy guards are private to this coordinator — a subscribe
    /// continuation and a concurrent feed refresh may each spawn a
    /// categorization / picks pass, but each pass locks `store` + `categories`
    /// / `picks` serially, so independent guards are safe (they only coalesce
    /// repeated spawns from the *same* source).
    pub(crate) fn new(
        store: Arc<Mutex<PodcastStore>>,
        rev: Arc<AtomicU64>,
        snapshot_signal: Option<SnapshotUpdateSignal>,
        categories: Arc<Mutex<HashMap<String, Vec<String>>>>,
        picks: Arc<Mutex<Vec<AgentPickSummary>>>,
        runtime: Arc<Runtime>,
    ) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            store,
            rev,
            snapshot_signal,
            categories,
            categorization_in_progress: Arc::new(AtomicBool::new(false)),
            picks,
            picks_score_in_progress: Arc::new(AtomicBool::new(false)),
            runtime,
        }
    }

    /// Record a fetch as in-flight under `request_id`. Called on the actor
    /// thread just before the async HTTP command is dispatched.
    pub(crate) fn register(&self, request_id: String, pending: PendingFeedFetch) {
        if let Ok(mut map) = self.pending.lock() {
            map.insert(request_id, pending);
        }
    }

    /// Apply an async HTTP report to its pending fetch. Runs on the platform
    /// transport thread — must never touch `*mut NmpApp`. Unknown / already-
    /// resolved `request_id`s are ignored (idempotent / D6).
    pub(crate) fn apply_report(&self, report: HttpReport) {
        let pending = match self.pending.lock() {
            Ok(mut map) => map.remove(&report.request_id),
            Err(_) => return,
        };
        let Some(pending) = pending else { return };
        match pending.mode {
            FeedFetchMode::Subscribe => self.apply_subscribe_result(pending, report.result),
        }
    }

    fn apply_subscribe_result(&self, pending: PendingFeedFetch, result: HttpResult) {
        let parsed = match handle_feed_response(
            &pending.url,
            pending.podcast_id,
            &result,
            None,
            Utc::now(),
        ) {
            Ok(FeedResult::Parsed { parsed, .. }) => parsed,
            // NotModified (a 304 on a known re-subscribe — existing episodes
            // were preserved by `mark_subscribed`) or a transport error: the
            // optimistic row is already visible, so there's nothing to merge.
            // Surfacing the error on the row is a tracked follow-up (BACKLOG).
            _ => return,
        };
        let etag_out = result.header("etag").map(str::to_owned);
        let lm_out = result.header("last-modified").map(str::to_owned);

        {
            let mut s = match self.store.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            let episodes = if pending.known {
                let existing: Vec<Episode> = s.episodes_for(pending.podcast_id).to_vec();
                merge_episodes(parsed.episodes, existing)
            } else {
                parsed.episodes
            };
            // `subscribe` upserts on id, so this replaces the optimistic
            // placeholder metadata with the parsed feed and keeps the follow
            // membership added at optimistic-insert time.
            s.subscribe(parsed.podcast, episodes);
            s.update_refresh_metadata(pending.podcast_id, etag_out, lm_out);
        } // store lock released before any re-projection / re-lock

        // Wake the projection so the hydrated episodes reach the shell.
        self.bump();

        // Mirror the synchronous subscribe's `auto_categorize` / `auto_refresh_picks`
        // triggers so freshly-arrived episodes pick up labels + a personalized
        // picks ranking. Both spawn their heavy work on the shared runtime and
        // touch only `store` / `categories` / `picks` / `rev` / signal — no app
        // pointer — so they are safe from the transport thread. They no-op
        // without a snapshot signal (headless / unit-test handles).
        if let Some(signal) = self.snapshot_signal.clone() {
            let _ = categorization_run_with_signal(
                &self.store,
                &self.categories,
                &self.rev,
                &self.runtime,
                &self.categorization_in_progress,
                signal.clone(),
            );
            let _ = picks_handle_refresh_with_signal(
                &self.store,
                &self.picks,
                &self.rev,
                &self.runtime,
                &self.picks_score_in_progress,
                signal,
            );
        }
    }

    fn bump(&self) {
        if let Some(signal) = &self.snapshot_signal {
            signal.bump();
        } else {
            self.rev.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
impl FeedFetchCoordinator {
    /// Throwaway coordinator over fresh state, for host-op-handler unit tests
    /// that construct a handler but never exercise the async feed-fetch path.
    pub(crate) fn new_test() -> Arc<Self> {
        Arc::new(Self::new(
            Arc::new(Mutex::new(PodcastStore::new())),
            Arc::new(AtomicU64::new(1)),
            None,
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime")),
        ))
    }
}

#[cfg(test)]
#[path = "feed_fetch_tests.rs"]
mod tests;
