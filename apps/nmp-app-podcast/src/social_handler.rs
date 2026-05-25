//! Stub handlers for the `podcast.fetch_contacts` action (feature #30).
//!
//! Owns the policy responses returned for the NIP-02 social graph until
//! the projection layer is wired into the NMP substrate contact store.
//! Today the NMP substrate already registers a kind:3 contact-list module
//! via `register_defaults`, but its store is not yet surfaced through the
//! podcast `PodcastUpdate.social` projection — that wire-up is tracked in
//! `docs/BACKLOG.md` (`pr-social-graph-nmp-store-wiring`).
//!
//! Kept in a sibling module to `host_op_handler` (mirroring
//! `comments_handler` next to it) so:
//!
//! 1. The stub policy has one canonical home, not a branch buried in the
//!    100+-line `HostOpHandler::handle` dispatch.
//! 2. The follow-up substrate-wiring PR can extend this module in place
//!    (adding a `&PodcastHandle` arg + projection-store writes) without
//!    re-threading the dispatch.
//!
//! Per D6, every variant returns a `serde_json::Value` envelope of the
//! `{"ok":true,...}` shape so synchronous dispatch always succeeds — the
//! `status: "nostr_pending"` discriminator tells the iOS shell to render
//! the empty/loading state without flipping its surface to an error.

use serde_json::json;

/// `{"ok":true,"status":"nostr_pending"}`.
///
/// Returned today for `podcast.fetch_contacts`. The iOS shell uses the
/// `nostr_pending` discriminator to keep the Social tab on its loading
/// state until the projection layer surfaces a real `SocialSnapshot` on
/// the next snapshot tick.
pub fn handle_fetch_contacts() -> serde_json::Value {
    json!({
        "ok": true,
        "status": "nostr_pending",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_contacts_returns_nostr_pending_envelope() {
        let v = handle_fetch_contacts();
        assert_eq!(v["ok"], true);
        assert_eq!(v["status"], "nostr_pending");
    }
}
