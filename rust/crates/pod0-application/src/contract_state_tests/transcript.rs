use pod0_domain::{
    ContentDigest, PodcastId, TranscriptArtifactInput, TranscriptArtifactSegmentInput,
    TranscriptSource,
};

use super::*;

#[test]
fn transcript_observation_requires_exact_context_and_is_terminal() {
    let request = request();
    let mut ledger = HostRequestLedger::default();
    assert!(ledger.register(request.clone()));
    assert!(ledger.is_transcript_request(request.request_id));

    let completed = observation(artifact(EpisodeId::from_parts(7, 1)));
    assert_eq!(
        ledger.accept_observation(&completed),
        ObservationAcceptance::Accepted
    );
    assert_eq!(
        ledger.accept_observation(&completed),
        ObservationAcceptance::Duplicate
    );

    let mut mismatched = HostRequestLedger::default();
    assert!(mismatched.register(request));
    assert_eq!(
        mismatched.accept_observation(&observation(artifact(EpisodeId::from_parts(7, 9)))),
        ObservationAcceptance::MismatchedPayload
    );
}

fn request() -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(7, 2),
        command_id: CommandId::from_parts(7, 3),
        cancellation_id: CancellationId::from_parts(7, 4),
        issued_revision: StateRevision::new(5),
        deadline_at: None,
        request: HostRequest::ExecuteTranscriptCapability {
            capability: crate::TranscriptCapabilityRequest::FetchPublisher {
                context: crate::TranscriptCapabilityContext {
                    episode_id: EpisodeId::from_parts(7, 1),
                    podcast_id: PodcastId::from_parts(7, 6),
                    source_revision: "audio-v1".to_owned(),
                },
                source_url: "https://example.test/transcript.vtt".to_owned(),
                mime_hint: Some("text/vtt".to_owned()),
                maximum_response_bytes: 4_096,
            },
        },
    }
}

fn observation(artifact: TranscriptArtifactInput) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: HostRequestId::from_parts(7, 2),
        cancellation_id: CancellationId::from_parts(7, 4),
        observed_request_revision: StateRevision::new(5),
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(6),
        observation: HostObservation::TranscriptCapabilityObserved {
            observation: crate::TranscriptCapabilityObservation::Completed {
                external_operation_id: None,
                provider_status: None,
                artifact,
            },
        },
    }
}

fn artifact(episode_id: EpisodeId) -> TranscriptArtifactInput {
    TranscriptArtifactInput {
        episode_id,
        podcast_id: PodcastId::from_parts(7, 6),
        source_revision: "audio-v1".to_owned(),
        source: TranscriptSource::Publisher,
        provider: None,
        source_payload_digest: ContentDigest::from_bytes([8; 32]),
        language: "en-US".to_owned(),
        generated_at: UnixTimestampMilliseconds::new(6),
        speakers: Vec::new(),
        segments: vec![TranscriptArtifactSegmentInput {
            text: "Calm by default.".to_owned(),
            start_milliseconds: 0,
            end_milliseconds: 1_000,
            speaker_id: None,
            words: Vec::new(),
        }],
    }
}
