use super::*;
use podcast_core::types::transcript::TranscriptKind;
fn make_nip74() -> NipF4DiscoveryEpisode {
    NipF4DiscoveryEpisode {
        d_tag: "ep-1".into(),
        title: "Pilot".into(),
        summary: "First".into(),
        published_at: 1_700_000_000,
        duration_secs: Some(120.5),
        image_url: Some("https://img.example/e.jpg".into()),
        audio_url: "https://m.example/ep.m4a".into(),
        audio_mime_type: Some("audio/mp4".into()),
        audio_sha256_hex: None,
        audio_size_bytes: None,
        show_a_tag: None,
        chapters_url: Some("https://c.example/c.json".into()),
        transcript_url: Some("https://t.example/t.vtt".into()),
        transcript_mime_type: Some("text/vtt".into()),
    }
}
#[test]
fn maps_every_supported_field() {
    let nip = make_nip74();
    let pid = PodcastId::generate();
    let ep = episode_to_episode(&nip, pid);
    assert_eq!(ep.podcast_id, pid);
    assert_eq!(ep.guid, "ep-1");
    assert_eq!(ep.title, "Pilot");
    assert_eq!(ep.description, "First");
    assert_eq!(ep.duration_secs, Some(120.5));
    assert_eq!(ep.enclosure_url.as_str(), "https://m.example/ep.m4a");
    assert_eq!(ep.enclosure_mime_type.as_deref(), Some("audio/mp4"));
    assert!(ep.image_url.is_some());
    assert!(ep.chapters_url.is_some());
    assert!(ep.publisher_transcript_url.is_some());
    assert!(matches!(ep.publisher_transcript_type, Some(TranscriptKind::Vtt)));
}
#[test]
fn id_is_stable_per_d_tag() {
    let nip = make_nip74();
    let pid = PodcastId::generate();
    let a = episode_to_episode(&nip, pid);
    let b = episode_to_episode(&nip, pid);
    assert_eq!(a.id, b.id);
}
#[test]
fn empty_title_yields_untitled_episode() {
    let mut nip = make_nip74();
    nip.title = String::new();
    let ep = episode_to_episode(&nip, PodcastId::generate());
    assert_eq!(ep.title, "Untitled Episode");
}

