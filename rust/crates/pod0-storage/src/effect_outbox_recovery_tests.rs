use pod0_application::{
    DurableEffectExecution, DurableTranscriptEffectRequest, ExternalEffectKind,
    TranscriptCapabilityContext, TranscriptCapabilityRequest, TranscriptProvider,
};
use pod0_domain::{
    CancellationId, CommandId, EpisodeId, HostRequestId, PodcastId, StateRevision,
    TranscriptAttemptId, TranscriptSubmissionFenceId, UnixTimestampMilliseconds,
};

use crate::EffectOutbox;
use crate::effect_outbox_tests::commit_effect_request;
use crate::recovery_test_support::Fixture;

#[test]
fn ambiguous_submission_waits_for_recovery_but_exact_reattach_can_reclaim() {
    let ambiguous = Fixture::new();
    ambiguous.migrate_to_current(54).unwrap();
    commit_effect_request(
        &ambiguous.store,
        ExternalEffectKind::TranscriptProvider,
        transcript_execution(false),
    );
    let outbox = EffectOutbox::open(&ambiguous.store).unwrap();
    outbox
        .claim_next_generated(time(1_000), 1_000)
        .unwrap()
        .unwrap();
    assert!(
        outbox
            .claim_next_generated(time(2_001), 1_000)
            .unwrap()
            .is_none()
    );
    assert_eq!(outbox.next_claim_at(time(2_001)).unwrap(), None);

    let exact = Fixture::new();
    exact.migrate_to_current(55).unwrap();
    commit_effect_request(
        &exact.store,
        ExternalEffectKind::TranscriptProvider,
        transcript_execution(true),
    );
    let outbox = EffectOutbox::open(&exact.store).unwrap();
    let first = outbox
        .claim_next_generated(time(1_000), 1_000)
        .unwrap()
        .unwrap();
    let second = outbox
        .claim_next_generated(time(2_001), 1_000)
        .unwrap()
        .unwrap();
    assert_eq!(second.request, first.request);
    assert_eq!(second.fence, first.fence + 1);
}

fn transcript_execution(recovery: bool) -> DurableEffectExecution {
    let context = TranscriptCapabilityContext {
        episode_id: EpisodeId::from_parts(10, 1),
        podcast_id: PodcastId::from_parts(10, 2),
        source_revision: "source-v1".into(),
    };
    let capability = if recovery {
        TranscriptCapabilityRequest::RecoverProvider {
            context,
            attempt_id: TranscriptAttemptId::from_parts(10, 3),
            submission_fence_id: TranscriptSubmissionFenceId::from_parts(10, 4),
            provider: TranscriptProvider::AssemblyAi,
            model: "universal-2".into(),
            external_operation_id: "provider-operation-1".into(),
            provider_status: Some("queued".into()),
            maximum_response_bytes: 1_024,
        }
    } else {
        TranscriptCapabilityRequest::SubmitProvider {
            context,
            attempt_id: TranscriptAttemptId::from_parts(10, 3),
            submission_fence_id: TranscriptSubmissionFenceId::from_parts(10, 4),
            provider: TranscriptProvider::AssemblyAi,
            model: "universal-2".into(),
            audio_url: "file:///audio.mp3".into(),
            maximum_response_bytes: 1_024,
        }
    };
    DurableEffectExecution::Transcript {
        request: DurableTranscriptEffectRequest {
            request_id: HostRequestId::from_parts(15, 2),
            command_id: CommandId::from_parts(15, 1),
            cancellation_id: CancellationId::from_parts(15, 3),
            issued_revision: StateRevision::INITIAL,
            deadline_at: Some(time(10_000)),
            capability,
        },
    }
}

const fn time(value: i64) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(value)
}
