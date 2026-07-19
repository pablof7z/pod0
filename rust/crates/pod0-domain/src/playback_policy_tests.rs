use crate::{
    CompletionStatus, DownloadArtifactStatus, EpisodeFeedMetadata, EpisodeId,
    EpisodeListeningState, EpisodeRecord, PlaybackSegment, PodcastId, TranscriptArtifactStatus,
    UnixTimestampMilliseconds, meaningful_listening_reached, playback_start_position,
    segment_reached, should_commit_position,
};

#[test]
fn resume_and_segment_start_policy_are_deterministic() {
    let mut episode = episode(120_000, 600_000);
    assert_eq!(playback_start_position(&episode, None), 120_000);
    episode.listening.resume_position_milliseconds = 598_000;
    assert_eq!(playback_start_position(&episode, None), 0);
    let segment = PlaybackSegment {
        start_position_milliseconds: Some(42_000),
        end_position_milliseconds: Some(84_000),
    };
    assert_eq!(playback_start_position(&episode, Some(segment)), 42_000);
    assert!(!segment_reached(83_999, Some(segment)));
    assert!(segment_reached(84_000, Some(segment)));
}

#[test]
fn meaningful_listening_uses_the_predeclared_five_minute_boundary() {
    assert!(!meaningful_listening_reached(299_999));
    assert!(meaningful_listening_reached(300_000));
}

#[test]
fn position_commit_policy_has_first_sample_boundary_and_thirty_second_cap() {
    let at = UnixTimestampMilliseconds::new(1_000_000);
    assert!(should_commit_position(0, 1_000, None, at, false));
    assert!(!should_commit_position(
        1_000,
        2_000,
        Some(at),
        UnixTimestampMilliseconds::new(1_029_999),
        false,
    ));
    assert!(should_commit_position(
        1_000,
        2_000,
        Some(at),
        UnixTimestampMilliseconds::new(1_030_000),
        false,
    ));
    assert!(should_commit_position(
        1_000,
        2_000,
        Some(at),
        UnixTimestampMilliseconds::new(1_000_001),
        true,
    ));
}

fn episode(resume: u64, duration: u64) -> EpisodeRecord {
    EpisodeRecord {
        episode_id: EpisodeId::from_parts(1, 1),
        podcast_id: PodcastId::from_parts(2, 2),
        publisher_guid: "guid".to_owned(),
        title: "Episode".to_owned(),
        description: String::new(),
        published_at: UnixTimestampMilliseconds::new(0),
        duration_milliseconds: Some(duration),
        enclosure_url: "https://example.test/audio.mp3".to_owned(),
        enclosure_mime_type: Some("audio/mpeg".to_owned()),
        image_url: None,
        feed_metadata: EpisodeFeedMetadata::default(),
        listening: EpisodeListeningState {
            resume_position_milliseconds: resume,
            completion: CompletionStatus::InProgress,
        },
        is_starred: false,
        download: DownloadArtifactStatus::Unavailable,
        transcript: TranscriptArtifactStatus::Unavailable,
    }
}
