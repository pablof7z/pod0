//! Tests for [`super::nostr_episodes`] — kind:54 episode observer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nmp_core::substrate::KernelEvent;

use crate::store::PodcastStore;

use super::*;

fn make_store() -> Arc<Mutex<PodcastStore>> {
    Arc::new(Mutex::new(PodcastStore::new()))
}

/// Build a synthetic `kind:54` `KernelEvent` with the given tags.
fn episode_event(
    id: &str,
    author: &str,
    tags: Vec<Vec<String>>,
) -> KernelEvent {
    KernelEvent {
        id: id.to_string(),
        author: author.to_string(),
        kind: KIND_NIP_F4_EPISODE,
        created_at: 1_700_000_000,
        tags,
        content: String::new(),
        relay_provenance: vec![],
    }
}

fn audio_tag(url: &str) -> Vec<String> {
    vec!["audio".to_string(), url.to_string(), "audio/mpeg".to_string()]
}

fn title_tag(value: &str) -> Vec<String> {
    vec!["title".to_string(), value.to_string()]
}

#[test]
fn observer_ignores_non_kind_54_events() {
    let store = make_store();
    let rev = Arc::new(AtomicU64::new(0));
    let obs = NostrEpisodesObserver::new(store.clone(), rev.clone());

    let mut ev = episode_event("ev", "pk", vec![audio_tag("https://a.example/ep.mp3")]);
    ev.kind = 10154; // wrong kind

    obs.on_kernel_event(&ev);
    assert_eq!(store.lock().unwrap().podcast_count(), 0);
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn observer_drops_unparseable_kind_54_events() {
    let store = make_store();
    let rev = Arc::new(AtomicU64::new(0));
    let obs = NostrEpisodesObserver::new(store.clone(), rev.clone());

    // kind:54 but no audio tag — parse fails (D6).
    obs.on_kernel_event(&episode_event("ev", "pk", vec![title_tag("Ep")]));

    assert_eq!(store.lock().unwrap().podcast_count(), 0);
    assert_eq!(rev.load(Ordering::Relaxed), 0);
}

#[test]
fn observer_creates_feedless_show_and_upserts_episode() {
    let store = make_store();
    let rev = Arc::new(AtomicU64::new(0));
    let obs = NostrEpisodesObserver::new(store.clone(), rev.clone());

    obs.on_kernel_event(&episode_event(
        "ev-1",
        "podcast-pk-1",
        vec![
            title_tag("Episode One"),
            audio_tag("https://audio.example/ep1.mp3"),
        ],
    ));

    let s = store.lock().unwrap();
    assert_eq!(s.podcast_count(), 1, "feedless show created");
    let id = s.podcast_id_for_pubkey("podcast-pk-1").expect("id");
    let eps = s.episodes_for(id);
    assert_eq!(eps.len(), 1);
    assert_eq!(eps[0].title, "Episode One");
    assert_eq!(eps[0].enclosure_url.as_str(), "https://audio.example/ep1.mp3");
    assert_eq!(rev.load(Ordering::Relaxed), 1);
}

#[test]
fn observer_appends_second_episode_same_show() {
    let store = make_store();
    let rev = Arc::new(AtomicU64::new(0));
    let obs = NostrEpisodesObserver::new(store.clone(), rev.clone());

    obs.on_kernel_event(&episode_event(
        "ev-1",
        "podcast-pk-1",
        vec![title_tag("Ep 1"), audio_tag("https://audio.example/ep1.mp3")],
    ));
    obs.on_kernel_event(&episode_event(
        "ev-2",
        "podcast-pk-1",
        vec![title_tag("Ep 2"), audio_tag("https://audio.example/ep2.mp3")],
    ));

    let s = store.lock().unwrap();
    assert_eq!(s.podcast_count(), 1, "still one show");
    let id = s.podcast_id_for_pubkey("podcast-pk-1").expect("id");
    let eps = s.episodes_for(id);
    assert_eq!(eps.len(), 2, "two distinct episodes");
}

#[test]
fn observer_deduplicates_same_event_id() {
    // Identical event (same event_id maps to same EpisodeId) arriving twice
    // must update in place, not append.
    let store = make_store();
    let rev = Arc::new(AtomicU64::new(0));
    let obs = NostrEpisodesObserver::new(store.clone(), rev.clone());

    let ev = episode_event(
        "same-ev",
        "pk-1",
        vec![title_tag("Ep"), audio_tag("https://audio.example/ep.mp3")],
    );
    obs.on_kernel_event(&ev);
    obs.on_kernel_event(&ev);

    let s = store.lock().unwrap();
    let id = s.podcast_id_for_pubkey("pk-1").expect("id");
    assert_eq!(s.episodes_for(id).len(), 1, "deduped in place");
    assert_eq!(rev.load(Ordering::Relaxed), 2, "both arrivals bump rev");
}

#[test]
fn observer_creates_separate_show_per_pubkey() {
    let store = make_store();
    let rev = Arc::new(AtomicU64::new(0));
    let obs = NostrEpisodesObserver::new(store.clone(), rev.clone());

    obs.on_kernel_event(&episode_event(
        "ev-a",
        "pk-a",
        vec![audio_tag("https://audio.example/a.mp3")],
    ));
    obs.on_kernel_event(&episode_event(
        "ev-b",
        "pk-b",
        vec![audio_tag("https://audio.example/b.mp3")],
    ));

    let s = store.lock().unwrap();
    assert_eq!(s.podcast_count(), 2);
    assert!(s.podcast_id_for_pubkey("pk-a").is_some());
    assert!(s.podcast_id_for_pubkey("pk-b").is_some());
}

#[test]
fn podcast_id_is_stable_for_same_pubkey() {
    // The PodcastId derived from a pubkey must be identical across separate
    // store instances (i.e. across app restarts). It is computed as a UUIDv5
    // over "nostr:show:<pubkey>".
    let store1 = make_store();
    let store2 = make_store();
    let rev = Arc::new(AtomicU64::new(0));

    let obs1 = NostrEpisodesObserver::new(store1.clone(), rev.clone());
    let obs2 = NostrEpisodesObserver::new(store2.clone(), rev.clone());

    let ev = episode_event("ev", "stable-pk", vec![audio_tag("https://a.example/ep.mp3")]);
    obs1.on_kernel_event(&ev);
    obs2.on_kernel_event(&ev);

    let id1 = store1.lock().unwrap().podcast_id_for_pubkey("stable-pk").unwrap();
    let id2 = store2.lock().unwrap().podcast_id_for_pubkey("stable-pk").unwrap();
    assert_eq!(id1, id2, "podcast id stable per pubkey");
}
