//! Pure download-queue state machine.
//!
//! [`DownloadQueue`] owns the canonical per-episode [`DownloadItem`] map
//! and projects [`DownloadReport`] events from the iOS download capability
//! into state mutations, optionally emitting follow-up [`DownloadCommand`]s
//! (e.g. `StartDownload` for the next queued item when a slot frees up).
//!
//! ## Pure
//!
//! `DownloadQueue` is **synchronous** and **side-effect-free**: no async,
//! no channels, no I/O, no clock. Every input is an explicit argument
//! (an `enqueue` call, a `cancel` call, a `handle_report` call). This is
//! deliberate: the FFI layer handles async dispatch and capability I/O;
//! this module is straight state-machine code that's cheap to unit-test.
//!
//! ## Doctrine
//!
//! * **D4 — single writer.** `DownloadQueue` is the sole writer of download
//!   state. The iOS capability owns `URLSessionDownloadTask` handles and a
//!   maps-by-episode-id for resume data, but the projection the UI reads
//!   comes from here.
//! * **D7 — Rust decides.** The iOS executor reports "ep-1 finished
//!   downloading"; this module decides whether that means the next queued
//!   item should start, whether the slot count is back under the
//!   concurrency cap, whether a cancelled item's resume token should be
//!   discarded. iOS doesn't ask "should I start ep-3 now?" — it reports
//!   `Completed` for ep-1, and Rust independently emits the follow-up
//!   `StartDownload` for ep-3 from its own queue state.
//! * **No retry policy here.** A `Failed` report transitions the item to
//!   `Failed` and frees the slot; whether to re-enqueue with backoff is
//!   a policy decision that lives in `podcast-feeds::refresh::policy`
//!   (M4.B).
//!
//! ## Concurrency cap
//!
//! `max_concurrent` is the number of `Active` + `Paused` items at any
//! point. Paused items still hold their slot — that's the trade-off for
//! resume-data continuity. If a caller wants to free the slot they must
//! explicitly `cancel`.

use std::collections::HashMap;

use crate::capability::{DownloadCommand, DownloadKind, DownloadReport};

mod delete;
mod item;
pub(crate) use delete::{
    apply_auto_delete_download, record_download_delete_failure, record_download_delete_success,
    remove_download_file, DownloadFileDeleteOutcome,
};
pub use item::{DownloadItem, DownloadItemState};

#[cfg(test)]
mod tests;

/// Default concurrency cap. Mirrors Apple's recommendation for background
/// `URLSession` (≤3 active discretionary tasks for a non-foreground app)
/// and what the legacy `EpisodeDownloadService` effectively allowed by way
/// of the OS scheduler.
pub const DEFAULT_MAX_CONCURRENT: usize = 3;

/// Pure projector over per-episode [`DownloadItem`] state.
///
/// All mutators take `&mut self` and return any follow-up
/// [`DownloadCommand`]s the FFI layer should dispatch back through the
/// capability. There is no internal clock or async primitive.
#[derive(Clone, Debug)]
pub struct DownloadQueue {
    /// Per-episode item records, keyed by `episode_id`. The map is the
    /// canonical state; the queue ordering for `Queued` items is captured
    /// in [`Self::queue_order`] so the state machine remains
    /// deterministic across `HashMap` iteration order changes.
    pub items: HashMap<String, DownloadItem>,
    /// FIFO of `episode_id`s for items currently in `Queued` state.
    /// Pulled from on `Completed` / `Failed` / `Cancelled` to start the
    /// next item.
    queue_order: Vec<String>,
    /// Maximum concurrent (Active + Paused) downloads. Defaults to
    /// [`DEFAULT_MAX_CONCURRENT`]; surfaceable via [`Self::with_capacity`]
    /// for tests.
    pub max_concurrent: usize,
}

impl Default for DownloadQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadQueue {
    /// Construct an empty queue with the default concurrency cap
    /// ([`DEFAULT_MAX_CONCURRENT`]).
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_CONCURRENT)
    }

    /// Construct an empty queue with an explicit concurrency cap.
    /// `max_concurrent` of zero is permitted but means no item ever
    /// starts; the caller is responsible for honouring whatever lower
    /// bound makes sense for the domain.
    #[must_use]
    pub fn with_capacity(max_concurrent: usize) -> Self {
        Self {
            items: HashMap::new(),
            queue_order: Vec::new(),
            max_concurrent,
        }
    }

    /// Number of items currently holding a concurrency slot
    /// (Active or Paused).
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.items.values().filter(|i| i.state.holds_slot()).count()
    }

    /// Number of items currently in `Queued` state.
    #[must_use]
    pub fn queued_count(&self) -> usize {
        self.queue_order.len()
    }

    /// Read-only access to an item by episode id.
    #[must_use]
    pub fn get(&self, episode_id: &str) -> Option<&DownloadItem> {
        self.items.get(episode_id)
    }

    /// Enqueue a new download for `episode_id`.
    ///
    /// If a non-terminal item with the same id already exists, this is a
    /// no-op (idempotence — re-issuing a `Start` for the same id should
    /// not double-up). If there's room under `max_concurrent`, the item
    /// goes straight to `Active` and we return `StartDownload`; otherwise
    /// it enters `Queued` and we return `None`.
    pub fn enqueue(
        &mut self,
        episode_id: impl Into<String>,
        url: impl Into<String>,
    ) -> Option<DownloadCommand> {
        self.enqueue_with_kind(episode_id, url, DownloadKind::Episode)
    }

    /// Enqueue a new download of an explicit [`DownloadKind`]. Same semantics
    /// as [`Self::enqueue`]; the kind rides on the item and every
    /// `StartDownload` the queue emits for it (including the slot-free re-emit
    /// in [`Self::start_next_queued`]) so the executor writes the file to the
    /// right place.
    pub fn enqueue_with_kind(
        &mut self,
        episode_id: impl Into<String>,
        url: impl Into<String>,
        kind: DownloadKind,
    ) -> Option<DownloadCommand> {
        let episode_id = episode_id.into();
        let url = url.into();

        // Idempotence: if an in-flight or paused item exists, do nothing.
        if let Some(existing) = self.items.get(&episode_id) {
            if !existing.state.is_terminal() {
                return None;
            }
        }

        // Wipe any terminal stale record so re-enqueuing after a Failed/
        // Cancelled starts fresh.
        self.items.remove(&episode_id);
        self.queue_order.retain(|e| e != &episode_id);

        let mut item = DownloadItem::queued_with_kind(&episode_id, &url, kind);
        if self.active_count() < self.max_concurrent {
            item.state = DownloadItemState::Active;
            self.items.insert(episode_id.clone(), item);
            Some(DownloadCommand::start_with_kind(
                url, episode_id, None, kind,
            ))
        } else {
            self.items.insert(episode_id.clone(), item);
            self.queue_order.push(episode_id);
            None
        }
    }

    /// Cancel an item by `episode_id`.
    ///
    /// * If the item is `Active` or `Paused`, returns
    ///   `CancelDownload` so the iOS executor can tear down the
    ///   `URLSessionDownloadTask`. The state moves to `Cancelled` only
    ///   when the matching `Cancelled` report arrives.
    /// * If the item is `Queued`, it's removed from the queue order and
    ///   marked `Cancelled` synchronously; no command is needed (the
    ///   capability never saw it).
    /// * Unknown / already-terminal items return `None`.
    pub fn cancel(&mut self, episode_id: &str) -> Option<DownloadCommand> {
        let item = self.items.get_mut(episode_id)?;
        match item.state {
            DownloadItemState::Active | DownloadItemState::Paused => {
                Some(DownloadCommand::cancel(episode_id.to_owned()))
            }
            DownloadItemState::Queued => {
                item.state = DownloadItemState::Cancelled;
                self.queue_order.retain(|e| e != episode_id);
                None
            }
            DownloadItemState::Completed
            | DownloadItemState::Failed
            | DownloadItemState::Cancelled => None,
        }
    }

    /// Pause an active download. Returns `PauseDownload` for the iOS
    /// executor when the item is `Active`; `None` for any other state.
    /// State moves to `Paused` only when the executor reports `Paused`.
    pub fn pause(&mut self, episode_id: &str) -> Option<DownloadCommand> {
        let item = self.items.get(episode_id)?;
        if item.state == DownloadItemState::Active {
            Some(DownloadCommand::PauseDownload {
                episode_id: episode_id.to_owned(),
            })
        } else {
            None
        }
    }

    /// Resume a paused download. Returns `ResumeDownload` for the iOS
    /// executor (which will rehydrate the resume data it stashed) when
    /// the item is `Paused`; `None` otherwise. The transition back to
    /// `Active` happens on the next `Progress` report.
    pub fn resume(&mut self, episode_id: &str) -> Option<DownloadCommand> {
        let item = self.items.get(episode_id)?;
        if item.state == DownloadItemState::Paused {
            Some(DownloadCommand::ResumeDownload {
                episode_id: episode_id.to_owned(),
            })
        } else {
            None
        }
    }

    /// Cancel every non-terminal item. Emits a single `CancelAll`
    /// command; per-item `Cancelled` reports follow from the executor
    /// and drive each item to its terminal `Cancelled` state via
    /// [`Self::handle_report`].
    ///
    /// Queued items (which the executor never saw) are moved to
    /// `Cancelled` synchronously and pruned from the order list.
    pub fn cancel_all(&mut self) -> Option<DownloadCommand> {
        let mut any_active = false;
        for item in self.items.values_mut() {
            if item.state == DownloadItemState::Queued {
                item.state = DownloadItemState::Cancelled;
            } else if item.state.holds_slot() {
                any_active = true;
            }
        }
        self.queue_order.clear();
        if any_active {
            Some(DownloadCommand::CancelAll)
        } else {
            None
        }
    }

    /// Project an inbound [`DownloadReport`] into queue state and emit
    /// any follow-up [`DownloadCommand`]s.
    ///
    /// The most common follow-up is a `StartDownload` for the next
    /// queued item when a slot frees up (on `Completed`, `Failed`,
    /// `Cancelled`). Multiple commands can theoretically be returned
    /// (if `max_concurrent` is bumped or the queue was over-filled by a
    /// concurrent mutation), so callers iterate the `Vec`.
    pub fn handle_report(&mut self, report: DownloadReport) -> Vec<DownloadCommand> {
        match report {
            DownloadReport::Progress {
                episode_id,
                bytes_downloaded,
                total_bytes,
            } => {
                if let Some(item) = self.items.get_mut(&episode_id) {
                    item.bytes_downloaded = bytes_downloaded;
                    if total_bytes.is_some() {
                        item.total_bytes = total_bytes;
                    }
                    // A `Progress` on a `Paused` item indicates the
                    // executor resumed faster than our pause command
                    // landed; reconcile.
                    if item.state == DownloadItemState::Paused {
                        item.state = DownloadItemState::Active;
                    }
                }
                Vec::new()
            }
            DownloadReport::Completed {
                episode_id,
                local_path,
            } => {
                if let Some(item) = self.items.get_mut(&episode_id) {
                    item.state = DownloadItemState::Completed;
                    item.local_path = Some(local_path);
                    item.error = None;
                    // Once complete, the totals are authoritative; if
                    // the server never reported Content-Length, fall
                    // back to bytes_downloaded so progress_fraction()
                    // reads 1.0.
                    if item.total_bytes.is_none() {
                        item.total_bytes = Some(item.bytes_downloaded);
                    }
                }
                self.start_next_queued()
            }
            DownloadReport::Failed { episode_id, error } => {
                if let Some(item) = self.items.get_mut(&episode_id) {
                    item.state = DownloadItemState::Failed;
                    item.error = Some(error);
                }
                self.start_next_queued()
            }
            DownloadReport::Cancelled { episode_id } => {
                if let Some(item) = self.items.get_mut(&episode_id) {
                    item.state = DownloadItemState::Cancelled;
                }
                self.start_next_queued()
            }
            DownloadReport::Paused {
                episode_id,
                bytes_downloaded,
            } => {
                if let Some(item) = self.items.get_mut(&episode_id) {
                    item.state = DownloadItemState::Paused;
                    item.bytes_downloaded = bytes_downloaded;
                    // Paused keeps the slot — don't start anything new.
                }
                Vec::new()
            }
        }
    }

    /// Pull the next FIFO `Queued` item up to the concurrency cap and
    /// emit `StartDownload` commands for each. Called from terminal
    /// transitions (`Completed`, `Failed`, `Cancelled`); a no-op when
    /// the queue is empty or the cap is already saturated.
    fn start_next_queued(&mut self) -> Vec<DownloadCommand> {
        let mut commands = Vec::new();
        while self.active_count() < self.max_concurrent {
            let Some(next_id) = self.queue_order.first().cloned() else {
                break;
            };
            self.queue_order.remove(0);
            let Some(item) = self.items.get_mut(&next_id) else {
                continue;
            };
            // Defensive: if the item was cancelled while still queued
            // (cancel() handles that synchronously), skip it.
            if item.state != DownloadItemState::Queued {
                continue;
            }
            item.state = DownloadItemState::Active;
            // Carry the item's kind so a queued *model* dispatched when a slot
            // frees still routes to the models directory (not the episode one).
            commands.push(DownloadCommand::start_with_kind(
                item.url.clone(),
                next_id,
                None,
                item.kind,
            ));
        }
        commands
    }
}
