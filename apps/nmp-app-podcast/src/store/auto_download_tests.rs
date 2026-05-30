use super::*;
use podcast_core::{Episode, PodcastId};
use url::Url;
use uuid::Uuid;

fn make_episode(podcast_id: PodcastId, guid: &str, url: &str) -> Episode {
    Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        guid,
        "Title",
        Url::parse(url).unwrap(),
        chrono::Utc::now(),
    )
}

/// Helper: call with wifi_only=false, is_on_wifi=true and return only ready downloads.
fn auto_dl(
    fresh: &[Episode],
    existing: &HashSet<String>,
    local: &HashMap<EpisodeId, String>,
    auto_on: bool,
) -> Vec<(EpisodeId, String)> {
    episodes_to_auto_download(fresh, existing, local, auto_on, false, true).0
}

#[test]
fn auto_download_off_does_not_queue_download() {
    let pid = PodcastId::generate();
    let fresh = vec![make_episode(pid, "g1", "https://ex.com/a.mp3")];
    let (ready, deferred) = episodes_to_auto_download(&fresh, &HashSet::new(), &HashMap::new(), false, false, true);
    assert!(ready.is_empty());
    assert!(deferred.is_empty());
}

#[test]
fn auto_download_on_queues_new_episodes() {
    let pid = PodcastId::generate();
    let ep_known = make_episode(pid, "known", "https://ex.com/known.mp3");
    let ep_new = make_episode(pid, "new", "https://ex.com/new.mp3");
    let fresh = vec![ep_known.clone(), ep_new.clone()];
    let mut existing = HashSet::new();
    existing.insert("known".to_string());
    let (ready, deferred) = episodes_to_auto_download(&fresh, &existing, &HashMap::new(), true, false, true);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].0, ep_new.id);
    assert_eq!(ready[0].1, "https://ex.com/new.mp3");
    assert!(deferred.is_empty());
}

#[test]
fn auto_download_skips_episodes_with_existing_local_path() {
    let pid = PodcastId::generate();
    let ep = make_episode(pid, "new", "https://ex.com/new.mp3");
    let fresh = vec![ep.clone()];
    let mut local = HashMap::new();
    local.insert(ep.id, "/tmp/already-here.mp3".to_string());
    let out = auto_dl(&fresh, &HashSet::new(), &local, true);
    assert!(out.is_empty());
}

#[test]
fn auto_download_preserves_input_order() {
    let pid = PodcastId::generate();
    let ep1 = make_episode(pid, "g1", "https://ex.com/1.mp3");
    let ep2 = make_episode(pid, "g2", "https://ex.com/2.mp3");
    let ep3 = make_episode(pid, "g3", "https://ex.com/3.mp3");
    let fresh = vec![ep1.clone(), ep2.clone(), ep3.clone()];
    let out = auto_dl(&fresh, &HashSet::new(), &HashMap::new(), true);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].0, ep1.id);
    assert_eq!(out[1].0, ep2.id);
    assert_eq!(out[2].0, ep3.id);
}

#[test]
fn auto_download_with_empty_fresh_list_returns_empty() {
    let out = auto_dl(&[], &HashSet::new(), &HashMap::new(), true);
    assert!(out.is_empty());
}

#[test]
fn auto_download_off_ignores_other_inputs() {
    let pid = PodcastId::generate();
    let ep = make_episode(pid, "g1", "https://ex.com/a.mp3");
    let out = auto_dl(&[ep], &HashSet::new(), &HashMap::new(), false);
    assert!(out.is_empty());
}

#[test]
fn auto_download_matches_local_paths_by_episode_id() {
    let pid = PodcastId::generate();
    let mut ep = make_episode(pid, "guid-stable", "https://ex.com/a.mp3");
    ep.id = EpisodeId::new(Uuid::nil());
    let mut local = HashMap::new();
    local.insert(ep.id, "/var/mobile/Downloads/a.mp3".to_string());
    let out = auto_dl(&[ep], &HashSet::new(), &local, true);
    assert!(out.is_empty());
}

// ── Wi-Fi gating tests ────────────────────────────────────────────────────

#[test]
fn wifi_only_on_wifi_queues_episodes() {
    let pid = PodcastId::generate();
    let ep = make_episode(pid, "new", "https://ex.com/new.mp3");
    let (ready, deferred) = episodes_to_auto_download(
        &[ep.clone()], &HashSet::new(), &HashMap::new(), true, true, true,
    );
    assert_eq!(ready.len(), 1);
    assert!(deferred.is_empty());
}

#[test]
fn wifi_only_on_cellular_defers_not_discards() {
    let pid = PodcastId::generate();
    let ep = make_episode(pid, "new", "https://ex.com/new.mp3");
    let (ready, deferred) = episodes_to_auto_download(
        &[ep.clone()], &HashSet::new(), &HashMap::new(), true, true, false,
    );
    assert!(ready.is_empty(), "must not dispatch on cellular when wifi-only");
    assert_eq!(deferred.len(), 1, "must defer (not discard) for later Wi-Fi dispatch");
    assert_eq!(deferred[0].0, ep.id);
}

#[test]
fn cellular_allowed_queues_on_cellular() {
    let pid = PodcastId::generate();
    let ep = make_episode(pid, "new", "https://ex.com/new.mp3");
    let (ready, deferred) = episodes_to_auto_download(
        &[ep.clone()], &HashSet::new(), &HashMap::new(), true, false, false,
    );
    assert_eq!(ready.len(), 1, "cellular-allowed must queue even without Wi-Fi");
    assert!(deferred.is_empty());
}

#[test]
fn auto_download_off_with_wifi_still_returns_empty() {
    let pid = PodcastId::generate();
    let ep = make_episode(pid, "new", "https://ex.com/new.mp3");
    let (ready, deferred) = episodes_to_auto_download(
        &[ep], &HashSet::new(), &HashMap::new(), false, true, true,
    );
    assert!(ready.is_empty());
    assert!(deferred.is_empty());
}
