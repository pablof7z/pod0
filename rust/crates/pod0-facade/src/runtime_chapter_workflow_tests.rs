use crate::runtime_chapter_workflow_test_support::*;
use crate::*;

#[test]
fn unresolved_request_rehydrates_with_the_exact_identity() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_100_000);
    dispatch_ensure(&facade, fixture.episode_id, 1);
    let first = one_request(&facade);

    let reopened = open(&fixture, first.lease.expires_at.value + 1);
    let recovered = one_request(&reopened);
    assert_eq!(recovered.request, first.request);
    assert!(recovered.lease.fence > first.lease.fence);
    let projection = workflows(&reopened, Some(fixture.episode_id));
    assert!(projection.failure.is_none());
    assert_eq!(projection.publisher.len(), 1);
    assert_eq!(
        projection.publisher[0].stage,
        PublisherChapterWorkflowStage::Requested
    );
}

#[test]
fn valid_response_commits_once_and_survives_restart() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_200_000);
    dispatch_ensure(&facade, fixture.episode_id, 2);
    let request = one_request(&facade);

    facade.record_leased_host_observation(leased_response(&request, 1, 200, valid_document()));
    let succeeded = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();
    assert_eq!(succeeded.stage, PublisherChapterWorkflowStage::Succeeded);
    assert!(succeeded.selected_artifact_id.is_some());
    let revision = facade
        .snapshot(workflow_request(Some(fixture.episode_id)))
        .state_revision;

    facade.record_leased_host_observation(leased_response(&request, 1, 200, valid_document()));
    assert_eq!(
        facade
            .snapshot(workflow_request(Some(fixture.episode_id)))
            .state_revision,
        revision
    );

    let reopened = open(&fixture, 1_800_000_200_000);
    assert!(reopened.next_leased_host_requests(64).is_empty());
    assert_eq!(
        workflows(&reopened, Some(fixture.episode_id)).publisher[0].selected_artifact_id,
        succeeded.selected_artifact_id
    );
}

#[test]
fn already_current_artifact_does_not_issue_another_request() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_250_000);
    dispatch_ensure(&facade, fixture.episode_id, 20);
    let request = one_request(&facade);
    facade.record_leased_host_observation(leased_response(&request, 1, 200, valid_document()));
    let current = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();

    dispatch_ensure(&facade, fixture.episode_id, 21);

    assert!(facade.next_leased_host_requests(64).is_empty());
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0],
        current
    );
}

#[test]
fn malformed_empty_missing_and_oversized_documents_fail_typed_in_rust() {
    let cases = [
        (
            200,
            b"{".to_vec(),
            PublisherChapterWorkflowFailureCode::InvalidDocument,
        ),
        (
            200,
            Vec::new(),
            PublisherChapterWorkflowFailureCode::InvalidDocument,
        ),
        (
            410,
            Vec::new(),
            PublisherChapterWorkflowFailureCode::NotFound,
        ),
    ];
    for (index, (status, bytes, expected)) in cases.into_iter().enumerate() {
        let fixture = publisher_fixture();
        let facade = open(&fixture, 1_800_000_260_000 + index as i64);
        dispatch_ensure(&facade, fixture.episode_id, 30 + index as u64);
        let request = one_request(&facade);
        facade.record_leased_host_observation(leased_response(&request, 1, status, bytes));
        let failed = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();
        assert_eq!(failed.stage, PublisherChapterWorkflowStage::Failed);
        assert_eq!(failed.failure.unwrap().code, expected);
    }

    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_270_000);
    dispatch_ensure(&facade, fixture.episode_id, 40);
    let request = one_request(&facade);
    facade.record_leased_host_observation(leased_response(
        &request,
        1,
        200,
        vec![0; MAX_PUBLISHER_CHAPTER_DOCUMENT_BYTES + 1],
    ));
    let failed = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();
    assert_eq!(failed.stage, PublisherChapterWorkflowStage::Failed);
    assert_eq!(
        failed.failure.unwrap().code,
        PublisherChapterWorkflowFailureCode::ResponseTooLarge
    );
}

#[test]
fn stale_request_revision_is_ignored_without_consuming_the_request() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_280_000);
    dispatch_ensure(&facade, fixture.episode_id, 50);
    let request = one_request(&facade);
    let mut stale = leased_response(&request, 1, 200, valid_document());
    stale.observation.observed_request_revision =
        StateRevision::new(request.request.issued_revision.value + 1);
    facade.record_leased_host_observation(stale);
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Requested
    );

    facade.record_leased_host_observation(leased_response(&request, 1, 200, valid_document()));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Succeeded
    );
}

#[test]
fn transient_failure_schedules_a_rust_timed_retry_and_rehydrates_it() {
    let now = 1_800_000_300_000;
    let fixture = publisher_fixture();
    let facade = open(&fixture, now);
    dispatch_ensure(&facade, fixture.episode_id, 3);
    let first = one_request(&facade);
    facade.record_leased_host_observation(publisher_observation(
        &first,
        1,
        HostObservation::Failed {
            code: HostFailureCode::Offline,
            safe_detail: None,
        },
    ));

    let retry_facade = open(&fixture, now + 30_000);
    let retry = one_request(&retry_facade);
    assert_ne!(retry.request.request_id, first.request.request_id);
    assert_eq!(
        retry.request.deadline_at,
        Some(UnixTimestampMilliseconds::new(now + 60_000))
    );
    assert!(matches!(
        retry.request.request,
        HostRequest::FetchPublisherChapters {
            not_before: Some(UnixTimestampMilliseconds { value }),
            ..
        } if value == now + 30_000
    ));
    let projection = workflows(&facade, Some(fixture.episode_id));
    assert_eq!(
        projection.publisher[0].stage,
        PublisherChapterWorkflowStage::RetryScheduled
    );
    assert_eq!(projection.publisher[0].attempt, 2);

    let reopened = open(&fixture, retry.lease.expires_at.value + 1);
    let recovered = one_request(&reopened);
    assert_eq!(recovered.request, retry.request);
}

#[test]
fn terminal_failure_can_be_retried_then_cancelled_through_typed_commands() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_400_000);
    dispatch_ensure(&facade, fixture.episode_id, 4);
    let first = one_request(&facade);
    facade.record_leased_host_observation(leased_response(&first, 1, 404, Vec::new()));
    let failed = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();
    assert_eq!(failed.stage, PublisherChapterWorkflowStage::Failed);
    assert_eq!(
        failed.failure.unwrap().code,
        PublisherChapterWorkflowFailureCode::NotFound
    );

    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(70, 5),
        cancellation_id: CancellationId::from_parts(71, 5),
        expected_revision: None,
        command: ApplicationCommand::RetryPublisherChapters {
            episode_id: fixture.episode_id,
            expected_workflow_revision: failed.workflow_revision,
        },
    });
    let retried_request = one_request(&facade);
    let retried = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(70, 6),
        cancellation_id: CancellationId::from_parts(71, 6),
        expected_revision: None,
        command: ApplicationCommand::CancelPublisherChapters {
            episode_id: fixture.episode_id,
            expected_workflow_revision: retried.workflow_revision,
        },
    });
    assert!(facade.next_leased_host_requests(64).is_empty());
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Cancelled
    );
    facade.record_leased_host_observation(leased_response(
        &retried_request,
        1,
        200,
        valid_document(),
    ));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Cancelled
    );
}

#[test]
fn no_chapters_url_produces_no_workflow_or_native_request() {
    let fixture = empty_fixture();
    let facade = open(&fixture, 1_800_000_600_000);
    dispatch_ensure(&facade, fixture.episode_id, 8);
    assert!(facade.next_leased_host_requests(64).is_empty());
    assert!(
        workflows(&facade, Some(fixture.episode_id))
            .publisher
            .is_empty()
    );
}
