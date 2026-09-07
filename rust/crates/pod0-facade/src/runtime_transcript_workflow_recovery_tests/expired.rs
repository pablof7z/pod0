use super::*;

#[test]
fn expired_submission_in_the_live_process_is_fenced_without_reposting() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_900_000_000_000)));
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(72, 20),
        cancellation_id: CancellationId::from_parts(72, 21),
        expected_revision: None,
        command: ApplicationCommand::EnsureTranscriptWorkflow {
            episode_id: fixture.episode_id,
            origin: TranscriptWorkflowOrigin::User,
            configuration: configuration(),
        },
    });
    let submission = transcript_request(&fixture.facade, "submission");

    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(submission.lease.expires_at.value + 1)));
    assert!(
        fixture
            .facade
            .next_leased_host_requests(u16::MAX)
            .into_iter()
            .all(|request| !matches!(
                request.request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            ))
    );
    assert_eq!(workflow_stage(&fixture), TranscriptWorkflowStage::Blocked);
    assert!(
        fixture
            .facade
            .state()
            .store
            .as_ref()
            .unwrap()
            .transcript_workflow(fixture.episode_id)
            .unwrap()
            .unwrap()
            .may_have_submitted
    );
}
