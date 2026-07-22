use pod0_domain::EpisodeId;

use crate::{
    TranscriptProvider, transcript_attempt_id, transcript_source_revision, transcript_speaker_id,
    transcript_submission_fence_id, transcript_workflow_id,
};

#[test]
fn identities_are_stable_and_attempt_zero_is_rejected() {
    let workflow = transcript_workflow_id(
        EpisodeId::from_bytes([4; 16]),
        "audio-v1",
        TranscriptProvider::ElevenLabsScribe,
        "scribe-v2",
    );
    assert_eq!(
        workflow,
        transcript_workflow_id(
            EpisodeId::from_bytes([4; 16]),
            "audio-v1",
            TranscriptProvider::ElevenLabsScribe,
            "scribe-v2",
        )
    );
    assert!(transcript_attempt_id(workflow, 0).is_none());
    let attempt = transcript_attempt_id(workflow, 1).unwrap();
    assert_ne!(attempt, transcript_attempt_id(workflow, 2).unwrap());
    assert_eq!(
        transcript_submission_fence_id(attempt),
        transcript_submission_fence_id(attempt)
    );
}

#[test]
fn speaker_identity_is_replay_stable_and_scoped_to_source() {
    let episode = EpisodeId::from_parts(4, 5);
    let first = transcript_speaker_id(episode, "audio-v1", "speaker-0").unwrap();
    assert_eq!(
        first,
        transcript_speaker_id(episode, "audio-v1", "speaker-0").unwrap()
    );
    assert_ne!(
        first,
        transcript_speaker_id(episode, "audio-v2", "speaker-0").unwrap()
    );
    assert_ne!(
        first,
        transcript_speaker_id(episode, "audio-v1", "speaker-1").unwrap()
    );
    assert!(transcript_speaker_id(episode, " audio-v1", "speaker-0").is_none());
    assert!(transcript_speaker_id(episode, "audio-v1", " ").is_none());
}

#[test]
fn source_revision_preserves_the_legacy_ios_audio_version_contract() {
    let revision = transcript_source_revision(
        "https://example.com/episode.mp3",
        Some("audio/mpeg"),
        Some(3_600_500),
    )
    .expect("valid media");
    assert_eq!(
        revision,
        "224dd593e74d189fd783abc3e5f9ef938ef5eee11132650bb0812d6bd6074a3e"
    );
    assert_eq!(
        transcript_source_revision("https://example.com/episode.mp3", None, None),
        Some("50eeb91c85bd35e364c4bec0c06be4d7aae8809e6def29b43521e2b920dbb136".into())
    );
    assert!(transcript_source_revision("not a URL", None, None).is_none());
}
