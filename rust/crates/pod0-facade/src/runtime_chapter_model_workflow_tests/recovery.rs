use super::*;

#[test]
fn provider_status_stream_recovers_by_operation_without_reposting() {
    let fixture = fixture();
    ensure(&fixture.facade, fixture.episode_id, 2);
    let request = model_request(&fixture.facade);
    let (episode_id, generation, submission_fence_id) = match request.request.request {
        HostRequest::ExecuteChapterModel {
            episode_id,
            generation,
            submission_fence_id,
            ..
        } => (episode_id, generation, submission_fence_id),
        _ => panic!("expected execution"),
    };
    for (sequence_number, status) in [(1, "queued"), (2, "running")] {
        assert_eq!(
            record_model_observation(
                &fixture.facade,
                &request,
                HostObservationEnvelope {
                    request_id: request.request.request_id,
                    cancellation_id: request.request.cancellation_id,
                    observed_request_revision: request.request.issued_revision,
                    sequence_number,
                    observed_at: UnixTimestampMilliseconds::new(
                        1_800_000_100_000 + sequence_number as i64,
                    ),
                    observation: HostObservation::ChapterModelProviderAccepted {
                        episode_id,
                        generation,
                        submission_fence_id,
                        update: ChapterModelProviderUpdate {
                            provider_operation_id: "provider-operation-1".into(),
                            provider_status: Some(status.into()),
                        },
                    },
                },
            ),
            HostObservationReceipt::Persisted {
                request_id: request.request.request_id,
                terminal: false,
            }
        );
    }

    let reopened = open(&fixture, request.lease.expires_at.value + 1);
    let recovery = model_request(&reopened);
    assert!(matches!(
        recovery.request.request,
        HostRequest::RecoverChapterModelOperation {
            ref provider_operation_id,
            ..
        } if provider_operation_id == "provider-operation-1"
    ));
}

#[test]
fn restart_after_claim_is_ambiguous_and_never_reposts() {
    let fixture = fixture();
    ensure(&fixture.facade, fixture.episode_id, 3);
    let _ = model_request(&fixture.facade);

    let reopened = open(&fixture, 1_800_000_200_000);
    assert!(reopened.next_leased_host_requests(64).is_empty());
    let model = workflows(&reopened, Some(fixture.episode_id))
        .model
        .pop()
        .unwrap();
    assert_eq!(model.stage, ModelChapterWorkflowStage::Ambiguous);
    assert!(model.allowed_actions.can_retry);
    let activity = pod0_storage::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap()
        .items;
    assert!(activity.iter().any(|item| {
        item.draft.actor == pod0_application::ActivityActor::Recovery
            && matches!(
                item.draft.fact,
                pod0_application::ActivityFact::DomainTransition {
                    kind: pod0_application::DomainTransitionKind::Chapter(
                        pod0_application::ChapterTransition::ModelWorkflowStateChanged
                    ),
                    ..
                }
            )
    }));
}

#[test]
fn exact_completion_retained_by_native_resolves_after_restart_before_lease_expiry() {
    let fixture = fixture();
    ensure(&fixture.facade, fixture.episode_id, 30);
    let request = model_request(&fixture.facade);
    let observation = completion(&request);

    let reopened = open(&fixture, request.lease.expires_at.value - 1);
    assert_eq!(
        workflows(&reopened, Some(fixture.episode_id)).model[0].stage,
        ModelChapterWorkflowStage::SubmissionAuthorized
    );
    assert_eq!(
        record_model_observation(&reopened, &request, observation),
        HostObservationReceipt::Persisted {
            request_id: request.request.request_id,
            terminal: true,
        }
    );
    assert_eq!(
        workflows(&reopened, Some(fixture.episode_id)).model[0].stage,
        ModelChapterWorkflowStage::Succeeded
    );
}

#[test]
fn malformed_paid_completion_is_durably_blocked_instead_of_replanned() {
    let fixture = fixture();
    ensure(&fixture.facade, fixture.episode_id, 4);
    let request = model_request(&fixture.facade);
    let mut observation = completion(&request);
    let HostObservation::ChapterModelCompleted { completion, .. } = &mut observation.observation
    else {
        unreachable!()
    };
    completion.completion = "{malformed".into();

    assert_eq!(
        record_model_observation(&fixture.facade, &request, observation),
        HostObservationReceipt::Persisted {
            request_id: request.request.request_id,
            terminal: true,
        }
    );
    let model = workflows(&fixture.facade, Some(fixture.episode_id))
        .model
        .pop()
        .unwrap();
    assert_eq!(model.stage, ModelChapterWorkflowStage::Blocked);
    assert_eq!(
        model.failure.unwrap().code,
        ModelChapterWorkflowFailureCode::QualificationRejected
    );
}
