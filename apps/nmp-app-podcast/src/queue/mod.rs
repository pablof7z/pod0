//! Playback queue ("Up Next").
//!
//! [`PlaybackQueue`] is a thin FIFO of episode-id strings. The front of the
//! queue is "what plays next"; new entries can be pushed to either end so the
//! UI can offer both "Play Next" (cut the line) and "Add to Queue" (queue at
//! the back). All ordering decisions live here; the snapshot projection
//! cross-references each id against [`crate::store::PodcastStore`] to build
//! the renderable [`crate::ffi::projections::EpisodeSummary`] rows.
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

/// FIFO ordering of episode ids the user has lined up to play after the
/// currently-loaded episode. Front of the deque is the next thing to play.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaybackQueue {
    /// Episode ids in play order. `order[0]` plays next when the active
    /// episode finishes (or when the user taps the "next" transport).
    order: Vec<String>,
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
        self.order.push(episode_id.to_owned());
    }

    /// Push an episode onto the front of the queue ("Play Next").
    ///
    /// If the id is already present, it is *moved* to the front rather than
    /// duplicated. Symmetric with [`Self::add_to_end`].
    pub fn add_to_front(&mut self, episode_id: &str) {
        self.remove(episode_id);
        self.order.insert(0, episode_id.to_owned());
    }

    /// Remove `episode_id` from anywhere in the queue. Silent no-op when the
    /// id isn't present.
    pub fn remove(&mut self, episode_id: &str) {
        self.order.retain(|id| id != episode_id);
    }

    /// Pop and return the next id in the queue, or `None` when empty.
    ///
    /// Used by the auto-advance path: when the active episode reports
    /// `Finished`, the kernel pops the next id and dispatches
    /// `AudioCommand::Load` + `Play` for it.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<String> {
        if self.order.is_empty() {
            None
        } else {
            Some(self.order.remove(0))
        }
    }

    /// Borrow the underlying ordering. Front-first.
    pub fn items(&self) -> &[String] {
        &self.order
    }

    /// Drop every queued id.
    pub fn clear(&mut self) {
        self.order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_queue_is_empty() {
        let q = PlaybackQueue::new();
        assert!(q.items().is_empty());
    }

    #[test]
    fn add_to_end_pushes_back() {
        let mut q = PlaybackQueue::new();
        q.add_to_end("a");
        q.add_to_end("b");
        q.add_to_end("c");
        assert_eq!(q.items(), &["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    }

    #[test]
    fn add_to_front_pushes_front() {
        let mut q = PlaybackQueue::new();
        q.add_to_front("a");
        q.add_to_front("b");
        q.add_to_front("c");
        // c was added last but to the front, so plays first.
        assert_eq!(q.items(), &["c".to_owned(), "b".to_owned(), "a".to_owned()]);
    }

    #[test]
    fn add_to_end_dedups_by_moving() {
        let mut q = PlaybackQueue::new();
        q.add_to_end("a");
        q.add_to_end("b");
        q.add_to_end("a"); // re-queue "a" at the back
        assert_eq!(q.items(), &["b".to_owned(), "a".to_owned()]);
    }

    #[test]
    fn add_to_front_dedups_by_moving() {
        let mut q = PlaybackQueue::new();
        q.add_to_end("a");
        q.add_to_end("b");
        q.add_to_front("b"); // cut the line — was at back, now at front
        assert_eq!(q.items(), &["b".to_owned(), "a".to_owned()]);
    }

    #[test]
    fn remove_existing_id() {
        let mut q = PlaybackQueue::new();
        q.add_to_end("a");
        q.add_to_end("b");
        q.add_to_end("c");
        q.remove("b");
        assert_eq!(q.items(), &["a".to_owned(), "c".to_owned()]);
    }

    #[test]
    fn remove_missing_id_is_noop() {
        let mut q = PlaybackQueue::new();
        q.add_to_end("a");
        q.remove("z");
        assert_eq!(q.items(), &["a".to_owned()]);
    }

    #[test]
    fn next_pops_front() {
        let mut q = PlaybackQueue::new();
        q.add_to_end("a");
        q.add_to_end("b");
        assert_eq!(q.next(), Some("a".to_owned()));
        assert_eq!(q.items(), &["b".to_owned()]);
        assert_eq!(q.next(), Some("b".to_owned()));
        assert!(q.items().is_empty());
    }

    #[test]
    fn next_on_empty_returns_none() {
        let mut q = PlaybackQueue::new();
        assert_eq!(q.next(), None);
    }

    #[test]
    fn clear_drops_everything() {
        let mut q = PlaybackQueue::new();
        q.add_to_end("a");
        q.add_to_end("b");
        q.add_to_end("c");
        q.clear();
        assert!(q.items().is_empty());
        // And `next` after `clear` returns None.
        assert_eq!(q.next(), None);
    }

    #[test]
    fn mixed_ops_preserve_ordering() {
        // Realistic scenario: user adds three to queue, decides one is urgent.
        let mut q = PlaybackQueue::new();
        q.add_to_end("ep-1");
        q.add_to_end("ep-2");
        q.add_to_end("ep-3");
        q.add_to_front("ep-3"); // dedup + move to front
        assert_eq!(
            q.items(),
            &["ep-3".to_owned(), "ep-1".to_owned(), "ep-2".to_owned()]
        );
        assert_eq!(q.next(), Some("ep-3".to_owned()));
        assert_eq!(
            q.items(),
            &["ep-1".to_owned(), "ep-2".to_owned()]
        );
    }

    #[test]
    fn default_is_empty() {
        let q = PlaybackQueue::default();
        assert_eq!(q, PlaybackQueue::new());
        assert!(q.items().is_empty());
    }
}
