//! Tests for [`super::discover_nostr`] — the NIP-F4 discovery interest, the
//! ref-counting identity, and the [`NostrDiscoveryObserver`] that turns
//! inbound `kind:10154` events into projected shows.
//!
//! Extracted from `discover_nostr.rs` to keep that file under the 500-line
//! hard limit (AGENTS.md).

use nmp_core::planner::{InterestLifecycle, InterestScope};
use nmp_core::substrate::KernelEvent;

use super::*;

// --- Interest declaration ---

#[test]
fn interest_targets_kind_10154_with_limit() {
    let interest = nostr_discovery_interest();
    assert_eq!(interest.id, nostr_discovery_interest_id());
    assert!(interest.shape.kinds.contains(&KIND_NIP_F4_SHOW));
    assert_eq!(interest.shape.limit, Some(NOSTR_DISCOVERY_LIMIT));
}

#[test]
fn interest_is_global_oneshot_and_indexer_routed() {
    let interest = nostr_discovery_interest();
    assert!(matches!(interest.scope, InterestScope::Global));
    assert!(matches!(interest.lifecycle, InterestLifecycle::OneShot));
    // Sparse kind must route through the indexer, not outbox relays.
    assert!(interest.is_indexer_discovery);
}

#[test]
fn interest_id_is_stable_across_calls() {
    assert_eq!(nostr_discovery_interest_id(), nostr_discovery_interest_id());
}

#[test]
fn interest_carries_no_relay_pin_so_nmp_routes_automatically() {
    // No relay URL is specified anywhere — NMP routes through its own pool.
    assert!(nostr_discovery_interest().shape.relay_pin.is_none());
}

// --- Ref-counting identity ---

#[test]
fn identity_owner_is_per_consumer_but_key_is_shared() {
    let a = nostr_discovery_identity("view-a");
    let b = nostr_discovery_identity("view-b");
    // Different consumers get distinct owners (independent claim/release)…
    assert_ne!(a.owner, b.owner);
    // …but share the dedup key + scope (one live subscription).
    assert_eq!(a.key, b.key);
    assert_eq!(a.scope, b.scope);
}

#[test]
fn identity_is_stable_for_the_same_consumer() {
    assert_eq!(
        nostr_discovery_identity("nostr-discover-view"),
        nostr_discovery_identity("nostr-discover-view"),
    );
}

#[test]
fn identity_scope_is_global() {
    assert_eq!(nostr_discovery_identity("v").scope, SubScope::Global);
}

// --- project_show ---

#[test]
fn project_show_preserves_every_field() {
    let show = NipF4Show {
        event_id: "ev".into(),
        author_pubkey: "pk".into(),
        title: "T".into(),
        description: Some("D".into()),
        feed_url: Some("https://x.example/rss".into()),
        artwork_url: Some("https://img.example/c.jpg".into()),
        categories: vec!["Tech".into()],
    };
    let projected = project_show(&show);
    assert_eq!(projected.event_id, "ev");
    assert_eq!(projected.author_pubkey, "pk");
    assert_eq!(projected.title, "T");
    assert_eq!(projected.description.as_deref(), Some("D"));
    assert_eq!(projected.feed_url.as_deref(), Some("https://x.example/rss"));
    assert_eq!(
        projected.artwork_url.as_deref(),
        Some("https://img.example/c.jpg")
    );
    assert_eq!(projected.categories, vec!["Tech".to_string()]);
}

// --- NostrDiscoveryObserver ---

fn make_slots() -> (Arc<Mutex<Vec<NostrShowSummary>>>, Arc<AtomicU64>) {
    (Arc::new(Mutex::new(Vec::new())), Arc::new(AtomicU64::new(0)))
}

/// Build a synthetic kernel event. Note the field is `author` (not `pubkey`)
/// and there is no `sig` — exactly the substrate `KernelEvent` shape the
/// observer receives on the ingest fan-out.
fn kernel_event(
    id: &str,
    author: &str,
    kind: u32,
    tags: Vec<Vec<String>>,
) -> KernelEvent {
    KernelEvent {
        id: id.to_string(),
        author: author.to_string(),
        kind,
        created_at: 0,
        tags,
        content: String::new(),
    }
}

fn title_tag(value: &str) -> Vec<String> {
    vec!["title".to_string(), value.to_string()]
}

#[test]
fn observer_projects_kind_10154_into_slot() {
    // This is the discriminating test: it proves the `author -> pubkey`
    // mapping works end-to-end. Build + the rest of the suite stay green even
    // if that mapping is wrong; only this test catches it.
    let (slot, rev) = make_slots();
    let obs = NostrDiscoveryObserver::new(slot.clone(), rev.clone());

    obs.on_kernel_event(&kernel_event(
        "ev1",
        "author-pk-1",
        KIND_NIP_F4_SHOW,
        vec![
            title_tag("My Show"),
            vec!["feed".into(), "https://feeds.example/a.rss".into()],
            vec!["summary".into(), "A great show".into()],
        ],
    ));

    let rows = slot.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "My Show");
    assert_eq!(rows[0].author_pubkey, "author-pk-1");
    assert_eq!(rows[0].feed_url.as_deref(), Some("https://feeds.example/a.rss"));
    assert_eq!(rows[0].description.as_deref(), Some("A great show"));
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn observer_ignores_non_10154_events() {
    let (slot, rev) = make_slots();
    let obs = NostrDiscoveryObserver::new(slot.clone(), rev.clone());

    // kind:1 short note — must be ignored.
    obs.on_kernel_event(&kernel_event("ev", "pk", 1, vec![title_tag("Note")]));

    assert!(slot.lock().unwrap().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn observer_drops_unparseable_10154_events() {
    let (slot, rev) = make_slots();
    let obs = NostrDiscoveryObserver::new(slot.clone(), rev.clone());

    // kind:10154 but no title tag and empty content -> parse fails (D6).
    obs.on_kernel_event(&kernel_event("ev", "pk", KIND_NIP_F4_SHOW, vec![]));

    assert!(slot.lock().unwrap().is_empty());
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn observer_dedups_by_author_pubkey() {
    // kind:10154 is replaceable: the same pubkey re-arrives (Replaced ingest,
    // or a second consumer re-Claiming). The slot must hold ONE row per
    // pubkey, updated in place — not two.
    let (slot, rev) = make_slots();
    let obs = NostrDiscoveryObserver::new(slot.clone(), rev.clone());

    obs.on_kernel_event(&kernel_event(
        "ev1",
        "same-pk",
        KIND_NIP_F4_SHOW,
        vec![title_tag("Original Title")],
    ));
    obs.on_kernel_event(&kernel_event(
        "ev2",
        "same-pk",
        KIND_NIP_F4_SHOW,
        vec![title_tag("Updated Title")],
    ));

    let rows = slot.lock().unwrap();
    assert_eq!(rows.len(), 1, "one row per author pubkey");
    assert_eq!(rows[0].title, "Updated Title", "row updated in place");
    // Two distinct upserts -> two rev bumps.
    assert_eq!(rev.load(Ordering::Relaxed), 2);
}

#[test]
fn observer_keeps_distinct_pubkeys_as_separate_rows() {
    let (slot, rev) = make_slots();
    let obs = NostrDiscoveryObserver::new(slot.clone(), rev.clone());

    obs.on_kernel_event(&kernel_event("e1", "pk-1", KIND_NIP_F4_SHOW, vec![title_tag("A")]));
    obs.on_kernel_event(&kernel_event("e2", "pk-2", KIND_NIP_F4_SHOW, vec![title_tag("B")]));

    assert_eq!(slot.lock().unwrap().len(), 2);
    assert_eq!(rev.load(Ordering::Relaxed), 2);
}

#[test]
fn observer_identical_rearrival_is_a_noop() {
    // An exact duplicate (same pubkey, same fields) must not churn rev — the
    // push frame should not fire for a no-change re-ingest.
    let (slot, rev) = make_slots();
    let obs = NostrDiscoveryObserver::new(slot.clone(), rev.clone());

    let ev = kernel_event(
        "ev",
        "pk",
        KIND_NIP_F4_SHOW,
        vec![title_tag("Stable"), vec!["feed".into(), "https://f.example/x.rss".into()]],
    );
    obs.on_kernel_event(&ev);
    obs.on_kernel_event(&ev);

    assert_eq!(slot.lock().unwrap().len(), 1);
    assert_eq!(rev.load(Ordering::Relaxed), 1, "identical re-arrival does not bump rev");
}
