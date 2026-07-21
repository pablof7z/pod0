use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::runtime_chapter_workflow_test_support::set_source;
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone)]
struct MutableClock(Arc<AtomicI64>);

impl pod0_application::Clock for MutableClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0.load(Ordering::SeqCst))
    }
}

#[test]
fn automatic_retry_uses_kernel_time_and_a_typed_native_wake() {
    let fixture = PlaybackFixture::new_with_transcript(true);
    set_source(&fixture, None);
    let time = Arc::new(AtomicI64::new(1_800_000_400_000));
    fixture
        .facade
        .state()
        .set_clock(Arc::new(MutableClock(Arc::clone(&time))));
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(82, 1),
        cancellation_id: CancellationId::from_parts(83, 1),
        expected_revision: None,
        command: ApplicationCommand::EnsureModelChapters {
            episode_id: fixture.episode_id,
            configured_model: "ollama:llama3.2".into(),
        },
    });
    let request = fixture
        .facade
        .next_host_requests(1)
        .into_iter()
        .next()
        .unwrap();
    let HostRequest::ExecuteChapterModel {
        episode_id,
        generation,
        submission_fence_id,
        ..
    } = request.request
    else {
        panic!("expected model execution")
    };
    assert_eq!(
        fixture
            .facade
            .record_host_observation(HostObservationEnvelope {
                request_id: request.request_id,
                cancellation_id: request.cancellation_id,
                observed_request_revision: request.issued_revision,
                sequence_number: 1,
                observed_at: UnixTimestampMilliseconds::new(time.load(Ordering::SeqCst)),
                observation: HostObservation::ChapterModelFailed {
                    episode_id,
                    generation,
                    submission_fence_id,
                    code: ChapterModelHostFailureCode::HttpResponse { status_code: 429 },
                    safe_detail: None,
                    retry_after_milliseconds: Some(30_000),
                },
            }),
        HostObservationReceipt::Persisted {
            request_id: request.request_id,
            terminal: true,
        }
    );
    let retry = fixture
        .facade
        .state()
        .store
        .as_ref()
        .unwrap()
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    assert_eq!(retry.not_before_ms, Some(1_800_000_430_000));

    let wake = fixture
        .facade
        .next_host_requests(1)
        .into_iter()
        .next()
        .unwrap();
    let HostRequest::ScheduleCoreWake { wake_at, reason } = wake.request else {
        panic!("retry must request an event-driven wake")
    };
    assert_eq!(wake_at.value, retry.not_before_ms.unwrap());
    time.store(wake_at.value, Ordering::SeqCst);
    assert_eq!(
        fixture
            .facade
            .record_host_observation(HostObservationEnvelope {
                request_id: wake.request_id,
                cancellation_id: wake.cancellation_id,
                observed_request_revision: wake.issued_revision,
                sequence_number: 1,
                observed_at: wake_at,
                observation: HostObservation::CoreWakeReached { reason },
            }),
        HostObservationReceipt::AcceptedTransient {
            request_id: wake.request_id,
        }
    );
    assert!(matches!(
        fixture
            .facade
            .next_host_requests(1)
            .into_iter()
            .next()
            .unwrap()
            .request,
        HostRequest::ExecuteChapterModel { generation: 2, .. }
    ));
}
