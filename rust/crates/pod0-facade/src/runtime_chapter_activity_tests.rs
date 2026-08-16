use pod0_application::{
    ActivityFact, ChapterTransition, DomainTransitionKind, ExternalEffectKind, HostRequest,
};
use pod0_storage::{ActivityStore, LibraryStore};

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn model_completion_consumes_only_its_lease_and_records_the_observation() {
    let fixture = PlaybackFixture::new_with_transcript(true);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(72, 920),
        cancellation_id: CancellationId::from_parts(73, 920),
        expected_revision: None,
        command: ApplicationCommand::EnsureModelChapters {
            episode_id: fixture.episode_id,
            configured_model: "ollama:llama3.2".into(),
        },
    });
    let leased = fixture
        .facade
        .next_leased_host_requests(8)
        .into_iter()
        .find(|item| {
            matches!(
                item.request.request,
                HostRequest::ExecuteChapterModel { .. }
            )
        })
        .unwrap();
    let HostRequest::ExecuteChapterModel {
        episode_id,
        generation,
        submission_fence_id,
        ..
    } = leased.request.request
    else {
        panic!("expected model execution")
    };
    let request_id = leased.request.request_id;
    let receipt = fixture.facade.record_leased_host_observation(
        pod0_application::LeasedHostObservationEnvelope {
            lease: leased.lease,
            observation: pod0_application::HostObservationEnvelope {
                request_id,
                cancellation_id: leased.request.cancellation_id,
                observed_request_revision: leased.request.issued_revision,
                sequence_number: 1,
                observed_at: UnixTimestampMilliseconds::new(1),
                observation: pod0_application::HostObservation::ChapterModelCompleted {
                    episode_id,
                    generation,
                    submission_fence_id,
                    completion: pod0_application::ChapterModelCompletionObservation {
                        completion: r#"{"chapters":[{"start":0,"title":"Opening"},{"start":30,"title":"Context"},{"start":60,"title":"Deep dive"},{"start":90,"title":"Close"}],"ads":[]}"#.into(),
                        provider: "ollama".into(),
                        model: "llama3.2:latest".into(),
                        prompt_tokens: None,
                        completion_tokens: None,
                        cached_tokens: None,
                        reasoning_tokens: None,
                        cost_microusd: None,
                        provider_operation_id: None,
                        provider_status: Some("completed".into()),
                        provider_generated_at: None,
                    },
                },
            },
        },
    );
    assert!(
        matches!(
            receipt,
            pod0_application::HostObservationReceipt::Persisted { terminal: true, .. }
        ),
        "{receipt:?}"
    );
    let committed = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap()
        .items;
    assert!(committed.iter().any(|item| {
        item.draft.host_request_id == Some(request_id)
            && matches!(
                item.draft.fact,
                ActivityFact::EffectObserved {
                    outcome: pod0_application::EffectOutcome::Succeeded,
                    ..
                }
            )
    }));
    assert!(committed.iter().any(|item| {
        item.draft.host_request_id == Some(request_id)
            && matches!(
                item.draft.fact,
                ActivityFact::InternalCommandAuthorized {
                    target: pod0_application::ActivityDomain::Chapter,
                    ..
                }
            )
    }));
    assert!(committed.iter().any(|item| {
        item.draft.origin == pod0_application::ActivityOrigin::InternalCommand
            && matches!(
                item.draft.fact,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Chapter(ChapterTransition::ArtifactAdopted),
                    ..
                }
            )
    }));
    assert!(
        LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .pending_internal_commands(100)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn model_provider_status_stream_is_logged_without_consuming_the_effect() {
    let fixture = PlaybackFixture::new_with_transcript(true);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(72, 921),
        cancellation_id: CancellationId::from_parts(73, 921),
        expected_revision: None,
        command: ApplicationCommand::EnsureModelChapters {
            episode_id: fixture.episode_id,
            configured_model: "ollama:llama3.2".into(),
        },
    });
    let leased = fixture
        .facade
        .next_leased_host_requests(8)
        .into_iter()
        .find(|item| {
            matches!(
                item.request.request,
                HostRequest::ExecuteChapterModel { .. }
            )
        })
        .unwrap();
    let HostRequest::ExecuteChapterModel {
        episode_id,
        generation,
        submission_fence_id,
        ..
    } = leased.request.request
    else {
        unreachable!()
    };
    for (sequence_number, status) in [(1, "queued"), (2, "running")] {
        let receipt = fixture.facade.record_leased_host_observation(
            pod0_application::LeasedHostObservationEnvelope {
                lease: leased.lease,
                observation: pod0_application::HostObservationEnvelope {
                    request_id: leased.request.request_id,
                    cancellation_id: leased.request.cancellation_id,
                    observed_request_revision: leased.request.issued_revision,
                    sequence_number,
                    observed_at: UnixTimestampMilliseconds::new(sequence_number as i64),
                    observation: pod0_application::HostObservation::ChapterModelProviderAccepted {
                        episode_id,
                        generation,
                        submission_fence_id,
                        update: pod0_application::ChapterModelProviderUpdate {
                            provider_operation_id: "operation-921".into(),
                            provider_status: Some(status.into()),
                        },
                    },
                },
            },
        );
        assert!(matches!(
            receipt,
            pod0_application::HostObservationReceipt::Persisted {
                terminal: false,
                ..
            }
        ));
    }
    let observed = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap()
        .items
        .into_iter()
        .filter(|item| {
            item.draft.host_request_id == Some(leased.request.request_id)
                && matches!(item.draft.fact, ActivityFact::EffectObserved { .. })
        })
        .count();
    assert_eq!(observed, 2);
}

#[test]
fn model_retry_schedules_then_authorizes_exactly_one_later_paid_submission() {
    let fixture = PlaybackFixture::new_with_transcript(true);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(72, 924),
        cancellation_id: CancellationId::from_parts(73, 924),
        expected_revision: None,
        command: ApplicationCommand::EnsureModelChapters {
            episode_id: fixture.episode_id,
            configured_model: "ollama:llama3.2".into(),
        },
    });
    let leased = fixture
        .facade
        .next_leased_host_requests(8)
        .into_iter()
        .find(|item| {
            matches!(
                item.request.request,
                HostRequest::ExecuteChapterModel { .. }
            )
        })
        .unwrap();
    let HostRequest::ExecuteChapterModel {
        episode_id,
        generation,
        submission_fence_id,
        ..
    } = leased.request.request
    else {
        unreachable!()
    };
    let receipt = fixture.facade.record_leased_host_observation(
        pod0_application::LeasedHostObservationEnvelope {
            lease: leased.lease,
            observation: pod0_application::HostObservationEnvelope {
                request_id: leased.request.request_id,
                cancellation_id: leased.request.cancellation_id,
                observed_request_revision: leased.request.issued_revision,
                sequence_number: 1,
                observed_at: UnixTimestampMilliseconds::new(1),
                observation: pod0_application::HostObservation::ChapterModelFailed {
                    episode_id,
                    generation,
                    submission_fence_id,
                    code: pod0_application::ChapterModelHostFailureCode::HttpResponse {
                        status_code: 429,
                    },
                    safe_detail: None,
                    retry_after_milliseconds: None,
                },
            },
        },
    );
    assert!(matches!(
        receipt,
        pod0_application::HostObservationReceipt::Persisted { terminal: true, .. }
    ));
    assert_eq!(model_effect_authorizations(&fixture), 1);

    let retry_at = LibraryStore::open_authoritative(&fixture.target)
        .unwrap()
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap()
        .not_before_ms
        .unwrap();
    let reopened = crate::runtime_chapter_workflow_test_support::open(&fixture, retry_at);
    let retry = reopened
        .next_leased_host_requests(8)
        .into_iter()
        .filter(|item| {
            matches!(
                item.request.request,
                HostRequest::ExecuteChapterModel { .. }
            )
        })
        .count();
    assert_eq!(retry, 1);
    assert_eq!(model_effect_authorizations(&fixture), 2);
}

fn model_effect_authorizations(fixture: &PlaybackFixture) -> usize {
    ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap()
        .items
        .into_iter()
        .filter(|item| {
            matches!(
                item.draft.fact,
                ActivityFact::EffectAuthorized {
                    kind: ExternalEffectKind::ModelChapterProvider,
                    ..
                }
            )
        })
        .count()
}
