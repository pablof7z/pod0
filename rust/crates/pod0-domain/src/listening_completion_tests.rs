use super::*;
use crate::listening_tests::golden_snapshot;

#[test]
fn completion_queue_and_forward_compatibility_invariants_are_explicit() {
    let mut snapshot = golden_snapshot();
    snapshot.episodes[0].listening.completion = CompletionStatus::Completed {
        cause: CompletionCause::NaturalEnd,
    };
    assert!(matches!(
        validate_listening_snapshot(snapshot.clone()),
        Err(ListeningDomainError::CompletedEpisodeHasResumePosition)
    ));
    snapshot.episodes[0].listening.resume_position_milliseconds = 0;
    snapshot.playback.sleep_mode = PlaybackSleepMode::Unsupported { wire_code: 77 };
    snapshot.episodes[0].transcript = TranscriptArtifactStatus::Unsupported { wire_code: 88 };
    assert!(validate_listening_snapshot(snapshot).is_ok());

    let mut legacy = golden_snapshot();
    legacy.episodes[0].listening.completion = CompletionStatus::Completed {
        cause: CompletionCause::LegacyPlayedFlag,
    };
    assert!(validate_listening_snapshot(legacy).is_ok());

    let mut duplicate = golden_snapshot();
    let mut duplicate_episode = duplicate.episodes[0].clone();
    duplicate_episode.episode_id = EpisodeId::from_parts(90, 91);
    duplicate.episodes.push(duplicate_episode);
    assert!(matches!(
        validate_listening_snapshot(duplicate),
        Err(ListeningDomainError::AmbiguousEpisodeIdentity)
    ));
}
