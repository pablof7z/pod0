use pod0_domain::EpisodeId;

use crate::{
    TranscriptProvider, transcript_attempt_id, transcript_speaker_id,
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
