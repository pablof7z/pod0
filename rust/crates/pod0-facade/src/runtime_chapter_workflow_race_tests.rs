use crate::runtime_chapter_workflow_test_support::*;
use crate::*;
use rusqlite::Connection;

#[test]
fn changed_source_rejects_stale_bytes_and_requests_the_new_url() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_500_000);
    dispatch_ensure(&facade, fixture.episode_id, 7);
    let stale = one_request(&facade);
    set_source(
        &fixture,
        Some("https://example.test/replacement-chapters.json"),
    );

    facade.record_host_observation(response(&stale, 1, 200, valid_document()));
    let replacement = one_request(&facade);
    assert_ne!(replacement.request_id, stale.request_id);
    assert!(matches!(
        replacement.request,
        HostRequest::FetchPublisherChapters { source_url, .. }
            if source_url == "https://example.test/replacement-chapters.json"
    ));
    assert!(
        selected_chapter(&facade, fixture.episode_id)
            .summary
            .is_none()
    );
}

#[test]
fn source_replacement_cancels_exact_old_request_and_late_bytes_cannot_delete_new_success() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_510_000);
    dispatch_ensure(&facade, fixture.episode_id, 70);
    let source_a = one_request(&facade);
    set_source(
        &fixture,
        Some("https://example.test/replacement-chapters.json"),
    );

    dispatch_ensure(&facade, fixture.episode_id, 71);
    let cancellations = facade.next_host_cancellations(64);
    assert_eq!(cancellations.len(), 1);
    assert_eq!(cancellations[0].request_id, source_a.request_id);
    assert_eq!(cancellations[0].cancellation_id, source_a.cancellation_id);
    let source_b = one_request(&facade);
    assert_ne!(source_b.request_id, source_a.request_id);

    facade.record_host_observation(response(&source_b, 1, 200, valid_document()));
    let succeeded = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();
    assert_eq!(succeeded.stage, PublisherChapterWorkflowStage::Succeeded);
    facade.record_host_observation(response(&source_a, 1, 200, valid_document()));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0],
        succeeded
    );
}

#[test]
fn source_replacement_ignores_old_response_before_new_response() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_520_000);
    dispatch_ensure(&facade, fixture.episode_id, 72);
    let source_a = one_request(&facade);
    set_source(&fixture, Some("https://example.test/source-b.json"));
    dispatch_ensure(&facade, fixture.episode_id, 73);
    let _ = facade.next_host_cancellations(64);
    let source_b = one_request(&facade);

    facade.record_host_observation(response(&source_a, 1, 200, valid_document()));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].request_id,
        Some(source_b.request_id)
    );
    facade.record_host_observation(response(&source_b, 1, 200, valid_document()));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Succeeded
    );
}

#[test]
fn removing_and_readding_same_source_never_reuses_request_identity() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_530_000);
    dispatch_ensure(&facade, fixture.episode_id, 74);
    let first = one_request(&facade);
    set_source(&fixture, None);
    dispatch_ensure(&facade, fixture.episode_id, 75);
    assert_eq!(facade.next_host_cancellations(64).len(), 1);
    assert!(
        workflows(&facade, Some(fixture.episode_id))
            .publisher
            .is_empty()
    );

    set_source(&fixture, Some("https://example.test/chapters.json"));
    dispatch_ensure(&facade, fixture.episode_id, 76);
    let readded = one_request(&facade);
    assert_ne!(readded.request_id, first.request_id);
    facade.record_host_observation(response(&first, 1, 200, valid_document()));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].request_id,
        Some(readded.request_id)
    );
}

#[test]
fn timed_out_platform_and_cancelled_observations_follow_typed_policy() {
    for (index, code) in [HostFailureCode::TimedOut, HostFailureCode::PlatformFailure]
        .into_iter()
        .enumerate()
    {
        let fixture = publisher_fixture();
        let facade = open(&fixture, 1_800_000_540_000 + index as i64);
        dispatch_ensure(&facade, fixture.episode_id, 80 + index as u64);
        let request = one_request(&facade);
        facade.record_host_observation(HostObservationEnvelope {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 1,
            observed_at: UnixTimestampMilliseconds::new(1),
            observation: HostObservation::Failed {
                code,
                safe_detail: None,
            },
        });
        let retry = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();
        assert_eq!(retry.stage, PublisherChapterWorkflowStage::RetryScheduled);
        assert!(matches!(
            retry.failure.unwrap().code,
            PublisherChapterWorkflowFailureCode::TimedOut
                | PublisherChapterWorkflowFailureCode::Transport
        ));
    }

    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_550_000);
    dispatch_ensure(&facade, fixture.episode_id, 82);
    let request = one_request(&facade);
    facade.record_host_observation(HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 1,
        observed_at: UnixTimestampMilliseconds::new(1),
        observation: HostObservation::Cancelled,
    });
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Cancelled
    );
}

#[test]
fn selection_revision_conflict_is_terminal_and_preserves_newer_selection() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_560_000);
    dispatch_ensure(&facade, fixture.episode_id, 83);
    let request = one_request(&facade);
    let replacement_document = br#"{"version":"1.2.0","chapters":[
      {"startTime":0,"title":"Newer selection"}
    ]}"#
    .to_vec();
    let store = pod0_storage::LibraryStore::open_authoritative(&fixture.target).unwrap();
    let replacement = publisher_artifact(&fixture, replacement_document);
    let replacement_id = store
        .commit_and_select_chapter(
            CommandId::from_parts(90, 1),
            StateRevision::INITIAL,
            replacement,
            1_800_000_560_001,
        )
        .unwrap()
        .artifact_id;

    facade.record_host_observation(response(&request, 1, 200, valid_document()));

    let failed = workflows(&facade, Some(fixture.episode_id)).publisher[0].clone();
    assert_eq!(failed.stage, PublisherChapterWorkflowStage::Failed);
    assert_eq!(
        failed.failure.unwrap().code,
        PublisherChapterWorkflowFailureCode::SelectionChanged
    );
    assert_eq!(
        store
            .selected_chapter_artifact(fixture.episode_id)
            .unwrap()
            .unwrap()
            .artifact
            .artifact_id,
        replacement_id
    );
    assert!(facade.next_host_requests(64).is_empty());
}

#[test]
fn accepted_observation_replays_after_same_process_write_failure() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_570_000);
    dispatch_ensure(&facade, fixture.episode_id, 84);
    let request = one_request(&facade);
    let lock = Connection::open(&fixture.target).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();

    facade.record_host_observation(response(&request, 1, 200, valid_document()));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Requested
    );
    lock.execute_batch("ROLLBACK").unwrap();

    assert!(facade.next_host_requests(64).is_empty());
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Succeeded
    );
}

#[test]
fn process_restart_after_http_success_reissues_until_durable_commit() {
    let fixture = publisher_fixture();
    let facade = open(&fixture, 1_800_000_580_000);
    dispatch_ensure(&facade, fixture.episode_id, 85);
    let request = one_request(&facade);
    let lock = Connection::open(&fixture.target).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();

    facade.record_host_observation(response(&request, 1, 200, valid_document()));
    assert_eq!(
        workflows(&facade, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Requested
    );
    drop(facade);
    lock.execute_batch("ROLLBACK").unwrap();

    let reopened = open(&fixture, 1_800_000_580_001);
    assert_eq!(one_request(&reopened), request);
    reopened.record_host_observation(response(&request, 1, 200, valid_document()));
    assert_eq!(
        workflows(&reopened, Some(fixture.episode_id)).publisher[0].stage,
        PublisherChapterWorkflowStage::Succeeded
    );
}
