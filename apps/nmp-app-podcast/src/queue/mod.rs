//! Playback queue ("Up Next").
//!
//! [`PlaybackQueue`] is a FIFO of playback intents. The front of the queue is
//! "what plays next"; new entries can be pushed to either end so the UI can
//! offer both "Play Next" (cut the line) and "Add to Queue" (queue at the
//! back). Entries persist/project as Rust-owned queue slots so duplicate
//! bounded segments can be removed/reordered without collapsing by episode id.
//!
//! ## Pure
//!
//! Like [`crate::player::PlayerActor`], this module is straight in-memory
//! state — no I/O, no clock, no async. Wrapped in `Arc<Mutex<…>>` on the
//! handle so the snapshot reader (main thread) and the action handler
//! (actor thread) share it.
//!
//! ## Doctrine
//!
//! * **D0** — episode ids live as opaque `String`s here; the podcast-domain
//!   resolution (id → enclosure URL → playback) is the action handler's job.
//! * **D6** — every mutation is total. `remove` and `next` on an empty queue
//!   are silent no-ops; duplicate `add_to_*` calls reposition rather than
//!   double-insert (a queued episode appearing twice in "Up Next" is a UI
//!   bug, not a feature).

use serde::{Deserialize, Serialize};

fn new_slot_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// One queued playback intent.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct QueuedPlaybackItem {
    /// Rust-owned stable queue slot identity. Distinct from `episode_id` so
    /// duplicate bounded segments can be addressed losslessly by the shell.
    #[serde(default = "new_slot_id")]
    pub slot_id: String,
    pub episode_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_secs: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_secs: Option<f64>,
}

impl QueuedPlaybackItem {
    pub fn whole_episode(episode_id: impl Into<String>) -> Self {
        Self {
            slot_id: new_slot_id(),
            episode_id: episode_id.into(),
            start_secs: None,
            end_secs: None,
        }
    }

    pub fn bounded(
        episode_id: impl Into<String>,
        start_secs: Option<f64>,
        end_secs: Option<f64>,
    ) -> Self {
        Self {
            slot_id: new_slot_id(),
            episode_id: episode_id.into(),
            start_secs,
            end_secs,
        }
    }

    pub fn with_slot_id(mut self, slot_id: impl Into<String>) -> Self {
        self.slot_id = slot_id.into();
        self
    }
}

/// FIFO ordering of playback intents the user has lined up to play after the
/// currently-loaded episode. Front of the deque is the next thing to play.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaybackQueue {
    /// Playback intents in play order. `order[0]` plays next when the active
    /// episode finishes (or when the user taps the "next" transport).
    order: Vec<QueuedPlaybackItem>,
}

impl PlaybackQueue {
    /// Construct an empty queue.
    pub fn new() -> Self {
        Self { order: Vec::new() }
    }

    /// Push an episode onto the back of the queue ("Add to Queue").
    ///
    /// If the id is already present, it is *moved* to the back rather than
    /// duplicated — the UI surface for "Up Next" must never show the same
    /// episode twice.
    pub fn add_to_end(&mut self, episode_id: &str) {
        self.remove(episode_id);
        self.order.push(QueuedPlaybackItem::whole_episode(episode_id));
    }

    /// Push an episode onto the front of the queue ("Play Next").
    ///
    /// If the id is already present, it is *moved* to the front rather than
    /// duplicated. Symmetric with [`Self::add_to_end`].
    pub fn add_to_front(&mut self, episode_id: &str) {
        self.remove(episode_id);
        self.order.insert(0, QueuedPlaybackItem::whole_episode(episode_id));
    }

    /// Push a bounded episode segment onto the back of the queue. Bounded
    /// entries intentionally do not deduplicate by episode id: the same episode
    /// can appear as multiple separate agent-selected segments.
    pub fn add_segment_to_end(
        &mut self,
        episode_id: &str,
        start_secs: Option<f64>,
        end_secs: Option<f64>,
    ) {
        self.order
            .push(QueuedPlaybackItem::bounded(episode_id, start_secs, end_secs));
    }

    /// Push a bounded episode segment onto the front of the queue.
    pub fn add_segment_to_front(
        &mut self,
        episode_id: &str,
        start_secs: Option<f64>,
        end_secs: Option<f64>,
    ) {
        self.order
            .insert(0, QueuedPlaybackItem::bounded(episode_id, start_secs, end_secs));
    }

    /// Remove `episode_id` from anywhere in the queue. Silent no-op when the
    /// id isn't present.
    pub fn remove(&mut self, episode_id: &str) {
        self.order.retain(|item| item.episode_id != episode_id);
    }

    /// Remove a single queue slot by Rust-owned slot id. This is the lossless
    /// path for duplicate bounded segments of the same episode.
    pub fn remove_slot(&mut self, slot_id: &str) {
        self.order.retain(|item| item.slot_id != slot_id);
    }

    /// Reorder existing queue slots by their Rust-owned slot ids. Unknown ids
    /// are ignored; existing slots omitted by the caller are appended in their
    /// prior relative order so a partial/stale shell reorder cannot drop data.
    pub fn reorder_by_slot_ids(&mut self, slot_ids: &[String]) {
        let mut remaining = std::mem::take(&mut self.order);
        let mut reordered = Vec::with_capacity(remaining.len());
        for slot_id in slot_ids {
            if let Some(index) = remaining.iter().position(|item| &item.slot_id == slot_id) {
                reordered.push(remaining.remove(index));
            }
        }
        reordered.extend(remaining);
        self.order = reordered;
    }

    /// Pop and return the next id in the queue, or `None` when empty.
    ///
    /// Used by the auto-advance path: when the active episode reports
    /// `Finished`, the kernel pops the next id and dispatches
    /// `AudioCommand::Load` + `Play` for it.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<QueuedPlaybackItem> {
        if self.order.is_empty() {
            None
        } else {
            Some(self.order.remove(0))
        }
    }

    /// Return the queue's episode ids in play order for snapshot projection,
    /// backwards-compatible persistence, and legacy tests.
    pub fn items(&self) -> Vec<String> {
        self.order.iter().map(|item| item.episode_id.clone()).collect()
    }

    /// Return the full queued playback intents in play order for persistence
    /// and queue-row projection. This is the canonical representation; `items`
    /// remains only for legacy callers that need episode ids.
    pub fn playback_items(&self) -> Vec<QueuedPlaybackItem> {
        self.order.clone()
    }

    /// Replace the queue with previously-persisted playback intents. Used only
    /// during cold-start hydration; preserves duplicate bounded segments.
    pub fn restore_items(&mut self, items: Vec<QueuedPlaybackItem>) {
        self.order = items;
    }

    /// Drop every queued id.
    pub fn clear(&mut self) {
        self.order.clear();
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
