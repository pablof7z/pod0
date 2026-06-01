use super::*;
use podcast_core::{Episode, Podcast};

fn store_with_episode() -> (PodcastStore, String) {
    let mut store = PodcastStore::new();
    let mut podcast = Podcast::new("Show");
    let podcast_id = podcast.id;
    podcast.feed_url = None;
    let ep = Episode::new(
        podcast_id,
        "https://example.com/feed.xml",
        "guid-1",
        "An Episode",
        url::Url::parse("https://example.com/e.mp3").unwrap(),
        chrono::Utc::now(),
    );
    let ep_id = ep.id.0.to_string();
    store.subscribe(podcast, vec![ep]);
    (store, ep_id)
}

#[test]
fn set_episode_summary_stamps_and_reads_back() {
    let (mut store, ep_id) = store_with_episode();
    assert_eq!(store.episode_summary(&ep_id), None);
    assert!(store.set_episode_summary(&ep_id, Some("A concise summary.".into())));
    assert_eq!(store.episode_summary(&ep_id), Some("A concise summary."));
}

#[test]
fn set_episode_summary_trims_and_clears_on_empty() {
    let (mut store, ep_id) = store_with_episode();
    assert!(store.set_episode_summary(&ep_id, Some("  padded  ".into())));
    assert_eq!(store.episode_summary(&ep_id), Some("padded"));
    // Whitespace-only / empty clears the field.
    assert!(store.set_episode_summary(&ep_id, Some("   ".into())));
    assert_eq!(store.episode_summary(&ep_id), None);
    assert!(store.set_episode_summary(&ep_id, None));
    assert_eq!(store.episode_summary(&ep_id), None);
}

#[test]
fn set_episode_summary_returns_false_for_unknown_id() {
    let (mut store, _ep_id) = store_with_episode();
    assert!(!store.set_episode_summary("not-a-real-id", Some("x".into())));
}

#[test]
fn episode_summary_inputs_collects_title_description_and_transcript() {
    let (mut store, ep_id) = store_with_episode();
    let inputs = store.episode_summary_inputs(&ep_id).unwrap();
    assert_eq!(inputs.title, "An Episode");
    assert_eq!(inputs.description, "");
    assert_eq!(inputs.transcript, None);

    store.set_transcript(ep_id.clone(), "spoken words here".into());
    let inputs = store.episode_summary_inputs(&ep_id).unwrap();
    assert_eq!(inputs.transcript.as_deref(), Some("spoken words here"));

    assert_eq!(store.episode_summary_inputs("missing"), None);
}
