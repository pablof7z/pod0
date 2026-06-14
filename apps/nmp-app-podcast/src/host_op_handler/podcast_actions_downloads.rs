//! Download-lifecycle host-op handlers for the podcast app.
//!
//! Split out of `podcast_actions.rs` (which kept owned-show, search, and
//! settings handlers) to keep both files under the 500-line hard limit. This
//! half owns everything that drives the unified [`crate::capability::DownloadQueue`]:
//! user-initiated and deferred-Wi-Fi downloads, on-device model downloads,
//! per-show auto-download policy + catch-up evaluation, and download deletion.
//!
//! Lock discipline (inherited from the parent module):
//! * Never hold a `PodcastStore` lock across a capability dispatch.
//! * Notifications + auto-downloads fire AFTER the store lock is released.

use uuid::Uuid;

use crate::host_op_handler::PodcastHostOpHandler;

impl PodcastHostOpHandler {
    pub(super) fn handle_dispatch_deferred_wifi_downloads(
        &self,
        correlation_id: &str,
    ) -> serde_json::Value {
        let pending = match self.state.library.store.lock() {
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
        match self.state.library.store.lock() {
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
            if let Ok(mut s) = self.state.library.store.lock() {
                s.add_pending_wifi_downloads(keep_pending);
            }
        }
        let count = dispatch_now.len();
        for (episode_id, url) in dispatch_now {
            let _ = self.start_episode_download(&episode_id, &url, correlation_id, true);
        }
        serde_json::json!({"ok": true, "dispatched": count})
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
            match self.state.library.store.lock() {
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
        if let Err(e) = self.start_episode_download(&episode_id_str, &url, correlation_id, false) {
            return serde_json::json!({"ok": false, "error": e});
        }
        serde_json::json!({"ok": true})
    }

    /// The single canonical path to start an episode download. Enqueues through
    /// the concurrency-bounded [`DownloadQueue`] (so auto-downloads and
    /// user-initiated downloads share one queue, honour `max_concurrent`, and
    /// surface in `DownloadQueueSnapshot`), records the request/start in the
    /// per-episode pipeline event log, and dispatches the resulting
    /// `StartDownload` when a slot was free. Fully idempotent: an episode
    /// already in flight or queued is a no-op (no duplicate event, no duplicate
    /// dispatch), so the cold-start / on-enable evaluate pass can re-run safely.
    ///
    /// `auto` distinguishes a policy-driven enqueue (`auto_download.queued`)
    /// from a user tap (`download.requested`) in the event log.
    ///
    /// [`DownloadQueue`]: crate::capability::DownloadQueue
    pub(super) fn start_episode_download(
        &self,
        episode_id: &str,
        url: &str,
        correlation_id: &str,
        auto: bool,
    ) -> Result<(), String> {
        use crate::capability::DownloadCommand;
        use crate::store::events::{stage, EventDetail, EventSeverity};

        let command = match self.state.playback.downloads.lock() {
            Ok(mut q) => {
                // Idempotence: skip an episode already active/queued/paused so a
                // repeated evaluate pass or a double-tap doesn't re-log or
                // re-dispatch. Terminal (failed/cancelled/completed) records are
                // re-enqueued fresh by `enqueue`.
                if let Some(item) = q.get(episode_id) {
                    if !item.state.is_terminal() {
                        return Ok(());
                    }
                }
                q.enqueue(episode_id.to_string(), url.to_string())
            }
            Err(_) => return Err("download_queue poisoned".into()),
        };
        self.bump_domain(crate::state::Domain::Downloads);

        if let Ok(mut s) = self.state.library.store.lock() {
            let (kind, summary): (&str, &str) = if auto {
                (stage::AUTO_DOWNLOAD_QUEUED, "Auto-download queued")
            } else {
                (stage::DOWNLOAD_REQUESTED, "Download requested")
            };
            s.emit_event(
                episode_id,
                kind,
                EventSeverity::Info,
                summary,
                vec![EventDetail::new("URL", url.to_string())],
            );
            if matches!(command, Some(DownloadCommand::StartDownload { .. })) {
                s.emit_event_simple(
                    episode_id,
                    stage::DOWNLOAD_STARTED,
                    EventSeverity::Info,
                    "Download started",
                );
            }
        }

        if let Some(cmd) = command {
            self.dispatch_download(&cmd, correlation_id)?;
        }
        Ok(())
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
        let command = match self.state.playback.downloads.lock() {
            Ok(mut q) => {
                q.enqueue_with_kind(model_id, url, crate::capability::DownloadKind::LocalModel)
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "download_queue poisoned"}),
        };
        self.bump_domain(crate::state::Domain::Downloads);
        if let Some(cmd) = command {
            if let Err(e) = self.dispatch_download(&cmd, correlation_id) {
                return serde_json::json!({"ok": false, "error": e});
            }
        }
        serde_json::json!({"ok": true})
    }

    pub(super) fn handle_set_auto_download(
        &self,
        podcast_id_str: String,
        enabled: bool,
        wifi_only: bool,
        correlation_id: &str,
    ) -> serde_json::Value {
        let uuid = match podcast_id_str.parse::<Uuid>() {
            Ok(u) => u,
            Err(_) => return serde_json::json!({"ok": false, "error": "invalid podcast_id"}),
        };
        let podcast_id = podcast_core::PodcastId::new(uuid);
        match self.state.library.store.lock() {
            Ok(mut s) => {
                s.set_auto_download(podcast_id, enabled);
                s.set_wifi_only(podcast_id, wifi_only);
                self.bump_domain(crate::state::Domain::Library);
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        }
        // Enabling on a show that already has a back catalog must backfill the
        // most-recent undownloaded episodes — the fresh-GUID refresh filter
        // skips every existing episode, so without this the toggle downloaded
        // nothing the user could see.
        if enabled {
            self.handle_evaluate_auto_downloads(correlation_id);
        }
        serde_json::json!({"ok": true})
    }

    /// Catch-up auto-download evaluation over the *current* library (op
    /// `auto_download_evaluate`). Dispatched on cold start (the foreground
    /// `RefreshAll` is skipped on first activation) and after enabling
    /// auto-download on a show. Queues each enabled show's most-recent
    /// undownloaded episodes (bounded by `AUTO_DOWNLOAD_BACKFILL_LIMIT`),
    /// deferring Wi-Fi-only shows while on cellular. Idempotent via the
    /// queue-backed [`Self::start_episode_download`].
    pub(super) fn handle_evaluate_auto_downloads(&self, correlation_id: &str) -> serde_json::Value {
        use crate::store::auto_download::AUTO_DOWNLOAD_BACKFILL_LIMIT;
        let (ready, deferred) = match self.state.library.store.lock() {
            Ok(s) => {
                let is_on_wifi = s.is_on_wifi();
                s.auto_download_backfill_candidates(is_on_wifi, AUTO_DOWNLOAD_BACKFILL_LIMIT)
            }
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        };
        for (episode_id, url) in &ready {
            let _ =
                self.start_episode_download(&episode_id.0.to_string(), url, correlation_id, true);
        }
        if !deferred.is_empty() {
            if let Ok(mut s) = self.state.library.store.lock() {
                use crate::store::events::{stage, EventSeverity};
                for (episode_id, _url) in &deferred {
                    s.emit_event_simple(
                        &episode_id.0.to_string(),
                        stage::AUTO_DOWNLOAD_DEFERRED,
                        EventSeverity::Info,
                        "Auto-download deferred until Wi-Fi",
                    );
                }
                s.add_pending_wifi_downloads(
                    deferred
                        .iter()
                        .map(|(id, url)| (id.0.to_string(), url.clone()))
                        .collect(),
                );
            }
        }
        // D8 / cold-start triage: iOS skips `RefreshAll` on first foreground
        // (see `KernelModel.lifecycleForeground` "Cold start skips RefreshAll"),
        // so episodes loaded from disk that are un-triaged or have a stale
        // (>TRIAGE_STALE_SECS) Ready entry would never be triaged at launch.
        // This op IS the cold-start seam, so kick a catch-up triage pass here.
        // Internally guarded by `triage_in_progress`/`episodes_needing_triage`
        // — a cheap no-op when nothing is due.
        self.state.inbox.maybe_enqueue_triage();
        serde_json::json!({"ok": true, "queued": ready.len(), "deferred": deferred.len()})
    }

    pub(super) fn handle_delete_download(&self, episode_id_str: String) -> serde_json::Value {
        use crate::download::{
            record_download_delete_failure, record_download_delete_success, remove_download_file,
            DownloadFileDeleteOutcome,
        };

        let Some((episode_id, path)) = (match self.state.library.store.lock() {
            Ok(s) => s.download_delete_candidate(&episode_id_str),
            Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
        }) else {
            return serde_json::json!({"ok": true});
        };

        match remove_download_file(&path) {
            DownloadFileDeleteOutcome::Removed | DownloadFileDeleteOutcome::AlreadyMissing => {
                let changed = match self.state.library.store.lock() {
                    Ok(mut s) => record_download_delete_success(
                        &mut s,
                        &episode_id_str,
                        episode_id,
                        "Downloaded file deleted",
                    ),
                    Err(_) => return serde_json::json!({"ok": false, "error": "store poisoned"}),
                };
                if changed {
                    self.bump_domain(crate::state::Domain::Library);
                }
                serde_json::json!({"ok": true})
            }
            DownloadFileDeleteOutcome::Failed(error) => {
                if let Ok(mut s) = self.state.library.store.lock() {
                    record_download_delete_failure(&mut s, &episode_id_str, &path, &error);
                }
                serde_json::json!({"ok": false, "error": format!("delete failed: {error}")})
            }
        }
    }
}
