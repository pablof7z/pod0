//! Shared projection invalidation signal.
//!
//! App-owned podcast slots use a local `rev` counter to gate the expensive
//! snapshot rebuild. When those slots mutate on the NMP actor thread, the
//! actor emits after the command. When they mutate from a background task or
//! direct FFI report, the mutation must also wake NMP's update sink so
//! host-registered snapshot projections re-emit without shell polling.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use nmp_core::actor::ActorCommand;
use nmp_core::CommandSender;

#[derive(Clone)]
pub(crate) struct SnapshotUpdateSignal {
    rev: Arc<AtomicU64>,
    actor_tx: CommandSender,
}

impl SnapshotUpdateSignal {
    pub(crate) fn new(rev: Arc<AtomicU64>, actor_tx: CommandSender) -> Self {
        Self { rev, actor_tx }
    }

    pub(crate) fn bump(&self) {
        self.rev.fetch_add(1, Ordering::Relaxed);
        let _ = self.actor_tx.send(ActorCommand::MarkChangedSinceEmit);
    }
}
