//! Pending-approvals projection helpers.
//!
//! The state itself lives inside [`crate::projections::ConversationActor`] —
//! conversations and approvals share a logical owner (an approval is
//! always attached to a conversation). This module exposes a thin
//! read-side façade so the snapshot serializer + UI can pull a
//! deterministic, sorted view without re-implementing the predicate.

use chrono::{DateTime, Utc};

use crate::types::PendingApproval;

/// Sorted, expiration-filtered view over a slice of [`PendingApproval`].
///
/// `now` is supplied by the caller so the projection layer can run in
/// the kernel tick (which owns the wall clock) without this helper
/// implicitly calling `Utc::now()`.
///
/// Approvals are returned in `requested_at` ascending order — the UI
/// renders oldest-first so users handle the queue in arrival order.
pub fn sorted_active_approvals(
    approvals: &[PendingApproval],
    now: DateTime<Utc>,
) -> Vec<PendingApproval> {
    let mut out: Vec<PendingApproval> = approvals
        .iter()
        .filter(|a| !a.is_expired_at(now))
        .cloned()
        .collect();
    out.sort_by_key(|a| a.requested_at);
    out
}

#[cfg(test)]
#[path = "approvals_tests.rs"]
mod tests;
