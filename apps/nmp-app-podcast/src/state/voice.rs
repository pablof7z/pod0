//! Voice substate — Step 12 of the god-root consolidation.
//!
//! Owns the two slots previously on the god-structs:
//!
//! * `voice_state_slot` — `Arc<Mutex<VoiceState>>` projection state.
//!   **Session** durability (reset on each process launch).
//! * `voice_conversation` — `VoiceConversationManager` (handle-only).
//!   Holds its own `Arc`s internally for store / voice_state / runtime.
//!   **Session** durability.
//!
//! ## Shutdown fence — CRITICAL
//!
//! `VoiceConversationManager` spawns Tokio tasks that dereference `*mut NmpApp`
//! (the capability dispatch back to iOS TTS). `nmp_app_podcast_unregister`
//! (in `ffi/snapshot.rs`) MUST call `shutdown()` on the manager before the
//! handle drops, so every in-flight task is aborted/joined before the
//! `NmpApp` allocation is freed.  After migration the unregister path calls
//! `reclaimed.state.voice.shutdown()` — **same ordering, same fence**.
//!
//! `shutdown()` on this substate delegates immediately to
//! `VoiceConversationManager::shutdown()`.  The ordering invariant is
//! preserved: unregister → shutdown → drop.
//!
//! ## Name clash avoidance
//!
//! The projection type in `ffi::projections` is also named `VoiceState`.
//! This substate is `VoiceSubstate` to avoid the clash; it is exported as
//! `voice::VoiceSubstate` and held at `PodcastAppState::voice`.
//!
//! ## File-length ceiling
//!
//! AGENTS.md hard limit is 500 lines.

use std::sync::{Arc, Mutex};

use crate::ffi::projections::VoiceState as VoiceProjection;
use crate::state::slot::Session;
use crate::state::{Infra, Slot};
use crate::store::PodcastStore;
use crate::voice_conversation::VoiceConversationManager;

/// Voice feature substate.
///
/// Constructed once in `PodcastAppState::new` and referenced via
/// `state.voice` on both seams.
pub struct VoiceSubstate {
    /// Kernel-side voice projection (listening / speaking / transcript flags).
    /// Written by `voice_handler::mutate_voice_state` on the actor thread and
    /// by `nmp_app_podcast_voice_report` from the platform thread.
    pub voice_state: Slot<VoiceProjection, Session>,
    /// LLM↔TTS conversation manager (handle-only). Holds the rolling turn
    /// history and dispatches `VoiceCommand::Speak` back to the iOS executor.
    ///
    /// ## Shutdown fence
    ///
    /// Call `shutdown()` BEFORE the `PodcastHandle` is dropped (i.e. from
    /// `nmp_app_podcast_unregister`).  This is the fence that prevents
    /// in-flight Tokio tasks from dereferencing the freed `NmpApp`.  See the
    /// module-level doc for the invariant description.
    pub(crate) voice_conversation: VoiceConversationManager,
    /// Rev + signal + runtime.  Scoped to `Domain::Voice` for push-projection
    /// deltas.  Used to bump the voice domain rev when a report arrives.
    pub(crate) infra: Infra,
}

impl VoiceSubstate {
    /// Production constructor — called from `PodcastAppState::new`.
    ///
    /// `app` is the raw `*mut NmpApp` pointer passed into `register`; it is
    /// forwarded to `VoiceConversationManager` so its spawned tasks can
    /// dispatch `VoiceCommand::Speak` back to iOS.
    pub fn new(
        infra: Infra,
        store: Arc<Mutex<PodcastStore>>,
        app: *mut nmp_ffi::NmpApp,
    ) -> Self {
        let voice_state_arc: Arc<Mutex<VoiceProjection>> =
            Arc::new(Mutex::new(VoiceProjection::default()));

        let voice_conversation = VoiceConversationManager::new(
            app,
            Arc::new(Mutex::new(Vec::new())),
            store,
            voice_state_arc.clone(),
            infra.runtime.clone(),
            infra.rev.clone(),
            infra.signal.clone(),
            infra.domain_revs.voice.clone(),
        );

        Self {
            voice_state: Slot::from_arc(voice_state_arc),
            voice_conversation,
            infra,
        }
    }

    /// Fence in-flight voice turns before the owning `NmpApp` frees.
    ///
    /// Delegates to [`VoiceConversationManager::shutdown`].  MUST be called
    /// from `nmp_app_podcast_unregister` BEFORE the handle drops — this is
    /// the UAF prevention fence described in the type-level docs on
    /// `VoiceConversationManager`.
    pub fn shutdown(&self) {
        self.voice_conversation.shutdown();
    }

    // ── Snapshot projection ───────────────────────────────────────────────

    /// Clone the current voice-state for snapshot projection.
    ///
    /// Returns `None` when the state equals the default (idle) so the
    /// snapshot omits the field for a fresh install / idle session.
    pub fn voice_snapshot(&self) -> Option<VoiceProjection> {
        self.voice_state.lock().ok().and_then(|v| {
            let snap = v.clone();
            (snap != VoiceProjection::default()).then_some(snap)
        })
    }
}

// SAFETY: `VoiceConversationManager` holds a `*mut NmpApp` which is not Send
// by default.  The manager documents its own safety contract (shutdown fence
// prevents UAF); `VoiceSubstate` simply forwards that contract — the pointer
// is only ever read, never mutated, and the `shutdown` fence is the caller's
// responsibility.
unsafe impl Send for VoiceSubstate {}
unsafe impl Sync for VoiceSubstate {}
