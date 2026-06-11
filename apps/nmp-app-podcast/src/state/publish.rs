//! Publish substate — Step 13 of the god-root consolidation.
//!
//! Owns the two slots previously on the god-structs:
//!
//! * `podcast_keys` — `Arc<Mutex<PodcastKeyStore>>`.
//!   **Persisted** durability (written to a sidecar file via the store;
//!   survives process restart so owned-podcast keys are not lost).
//! * `publish_state` — `Arc<Mutex<HashMap<String, OwnedPublishState>>>`.
//!   **Session** durability (in-memory diagnostic map; rebuilt from the
//!   first publish action after each cold launch).
//!
//! ## Access pattern
//!
//! Both seams reach these fields via `state.publish.podcast_keys` and
//! `state.publish.publish_state` (replacing the parallel god-struct
//! fields `handler.podcast_keys` / `handle.podcast_keys`).
//!
//! The free functions in `host_op_publish.rs` and
//! `host_op_publish_lifecycle.rs` continue to take a `&PodcastHostOpHandler`
//! argument — they are NOT converted to methods in this step (the publish
//! logic needs `handler.store`, `handler.app`, `handler.rev`, etc.).  The
//! canonical access path is simply updated from `handler.podcast_keys` →
//! `handler.state.publish.podcast_keys`.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::ffi::handle::OwnedPublishState;
use crate::state::slot::{Persisted, Session};
use crate::state::{Infra, Slot};
use crate::store::{PodcastKeyStore, PodcastStore};

/// Publish substate — owns the NIP-F4 owned-podcast key store and the
/// per-podcast diagnostic publish map.
pub struct PublishState {
    /// Per-podcast Nostr keypairs for NIP-F4 owned podcasts (features
    /// #27/#28). Written by `podcast.publish.create_owned_podcast` and
    /// cleared by `remove_owned_podcast`; read by every other publish op.
    ///
    /// **Persisted** — keys must survive an app restart so previously
    /// owned podcasts don't lose their signing identity on cold launch.
    pub podcast_keys: Slot<PodcastKeyStore, Persisted>,
    /// Diagnostic publish state per podcast (last show event JSON +
    /// last-published timestamp). Surfaced via `OwnedPodcastInfo` so the
    /// iOS shell can render "last published at …" without a separate
    /// FFI accessor. Keyed by `podcast_id` UUID string.
    ///
    /// **Session** — reset on each process launch; the real source of truth
    /// is the Nostr relay + the signed events, not this diagnostic cache.
    pub publish_state: Slot<HashMap<String, OwnedPublishState>, Session>,
    /// Rev + signal + runtime (unused directly; kept for future substate
    /// methods that might bump the snapshot rev).
    #[allow(dead_code)]
    infra: Infra,
}

impl PublishState {
    /// Construct from shared infra and a store clone.
    ///
    /// `store` is accepted to match the constructor convention of every
    /// other substate; it is not currently used inside `PublishState`
    /// itself (the podcast-key persistence layer is invoked from the
    /// handler free functions, not from the substate).
    pub fn new(infra: Infra, _store: Arc<Mutex<PodcastStore>>) -> Self {
        Self {
            podcast_keys: Slot::new(PodcastKeyStore::new()),
            publish_state: Slot::new(HashMap::new()),
            infra,
        }
    }
}
