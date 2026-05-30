use super::*;
use chrono::{TimeZone, Utc};
use podcast_core::types::episode::Episode;
use podcast_core::types::podcast::PodcastId;
use podcast_core::types::transcript::TranscriptKind;
use url::Url;
fn fixture() -> Episode {
    let mut ep = Episode::new(
        PodcastId::generate(),
        "https://media.example/feed.xml",
        "publisher-guid",
        "Pilot",
        Url::parse("https://media.example/ep.m4a").unwrap(),
        Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
    );
    ep.description = "First episode".into();
    ep.duration_secs = Some(1800.0);
    ep
}
#[test]
fn minimal_episode_emits_required_tags() {
    let ep = fixture();
    let tags = episode_to_episode_tags(&ep);
    let names: Vec<&str> = tags.iter().filter_map(|t| t.first().map(String::as_str)).collect();
    assert_eq!(names, vec!["title", "description", "duration", "audio"]);
    assert_eq!(tags[0], vec!["title".to_string(), "Pilot".into()]);
    assert_eq!(tags[1], vec!["description".to_string(), "First episode".into()]);
    assert_eq!(tags[2], vec!["duration".to_string(), "1800".into()]);
    assert_eq!(
        tags[3],
        vec![
            "audio".to_string(),
            "https://media.example/ep.m4a".into(),
            "audio/mp4".into()
        ]
    );
}
#[test]
fn audio_uses_default_mime_when_not_supplied() {
    let ep = fixture();
    let tags = episode_to_episode_tags(&ep);
    let audio = tags
        .iter()
        .find(|t| t.first().map(String::as_str) == Some("audio"))
        .expect("audio present");
    assert_eq!(audio[1], "https://media.example/ep.m4a");
    assert_eq!(audio[2], "audio/mp4");
}
#[test]
fn audio_uses_supplied_url_override_when_present() {
    // M8: a Blossom URL override replaces the RSS enclosure URL in the
    // `audio` tag while still falling back to the episode mime.
    let ep = fixture();
    let imeta_info = ImetaInfo {
        url: Some("https://blossom.example/blob.mp3".into()),
        ..Default::default()
    };
    let tags = episode_to_episode_tags_with_imeta(&ep, &imeta_info);
    let audio = tags
        .iter()
        .find(|t| t.first().map(String::as_str) == Some("audio"))
        .expect("audio present");
    assert_eq!(audio[1], "https://blossom.example/blob.mp3");
}
#[test]
fn audio_uses_supplied_mime_when_present() {
    let ep = fixture();
    let imeta_info = ImetaInfo {
        mime_type: Some("audio/m4a".into()),
        ..Default::default()
    };
    let tags = episode_to_episode_tags_with_imeta(&ep, &imeta_info);
    let audio = tags
        .iter()
        .find(|t| t.first().map(String::as_str) == Some("audio"))
        .expect("audio present");
    assert_eq!(audio[1], "https://media.example/ep.m4a");
    assert_eq!(audio[2], "audio/m4a");
}
#[test]
fn full_episode_includes_chapters_and_transcript() {
    let mut ep = fixture();
    ep.chapters_url = Some(Url::parse("https://c.example/c.json").unwrap());
    ep.publisher_transcript_url = Some(Url::parse("https://t.example/t.vtt").unwrap());
    ep.publisher_transcript_type = Some(TranscriptKind::Vtt);
    ep.image_url = Some(Url::parse("https://img.example/cover.jpg").unwrap());
    let tags = episode_to_episode_tags(&ep);
    let chapters = tags.iter().find(|t| t.first().map(String::as_str) == Some("chapters")).expect("chapters tag");
    assert_eq!(chapters[1], "https://c.example/c.json");
    assert_eq!(chapters[2], "application/json+chapters");
    let transcript = tags.iter().find(|t| t.first().map(String::as_str) == Some("transcript")).expect("transcript tag");
    assert_eq!(transcript[1], "https://t.example/t.vtt");
    assert_eq!(transcript[2], "text/vtt");
    let image = tags.iter().find(|t| t.first().map(String::as_str) == Some("image")).expect("image tag");
    assert_eq!(image[1], "https://img.example/cover.jpg");
}
