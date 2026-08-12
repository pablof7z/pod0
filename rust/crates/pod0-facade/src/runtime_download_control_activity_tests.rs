use std::sync::Arc;

use pod0_application::{
    ActivityFact, DomainTransitionKind, DownloadTransition, RequestDisposition,
    RequestRejectionReason,
};
use pod0_storage::ActivityStore;

use crate::runtime_download_workflow_tests::{
    FixedClock, dispatch, observe_wifi, request_download, staged_observation, workflows,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn cancel_and_remove_are_typed_atomic_transitions_with_durable_rejections() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    observe_wifi(&fixture.facade, 1);
    let environment = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(
            pod0_application::CommandActivityIdentity::new(CommandId::from_parts(70, 1))
                .correlation_id(),
            None,
            20,
        )
        .unwrap();
    assert!(environment.items.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Download(DownloadTransition::EnvironmentChanged),
            ..
        }
    )));
    request_download(&fixture, 2);
    let start = fixture.facade.next_host_requests(20).pop().unwrap();
    let revision = workflows(&fixture.facade, fixture.episode_id).workflows[0].workflow_revision;
    dispatch(
        &fixture.facade,
        3,
        ApplicationCommand::CancelEpisodeDownload {
            episode_id: fixture.episode_id,
            expected_workflow_revision: revision,
        },
    );
    assert_transition(&fixture, 3, DownloadTransition::DesiredStateChanged);
    assert_transition(&fixture, 3, DownloadTransition::AttemptStateChanged);

    dispatch(
        &fixture.facade,
        4,
        ApplicationCommand::CancelEpisodeDownload {
            episode_id: fixture.episode_id,
            expected_workflow_revision: revision,
        },
    );
    assert_rejection(&fixture, 4, RequestRejectionReason::RevisionConflict);

    request_download(&fixture, 5);
    let request = fixture
        .facade
        .next_host_requests(20)
        .into_iter()
        .find(|request| matches!(request.request, HostRequest::StartEpisodeDownload { .. }))
        .unwrap();
    let media = fixture
        .target
        .parent()
        .unwrap()
        .join("remove-activity.media");
    let bytes = b"remove activity media";
    std::fs::write(&media, bytes).unwrap();
    fixture.facade.record_host_observation(staged_observation(
        &request,
        1,
        media.to_string_lossy().into_owned(),
        bytes.len() as u64,
    ));
    let succeeded = workflows(&fixture.facade, fixture.episode_id).workflows[0].workflow_revision;
    dispatch(
        &fixture.facade,
        6,
        ApplicationCommand::RemoveEpisodeDownload {
            episode_id: fixture.episode_id,
            expected_workflow_revision: succeeded,
        },
    );
    assert_transition(&fixture, 6, DownloadTransition::DesiredStateChanged);
    assert_transition(&fixture, 6, DownloadTransition::AttemptStateChanged);
    drop(start);
}

fn facts(fixture: &PlaybackFixture) -> Vec<pod0_application::CommittedActivityFact> {
    ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 200)
        .unwrap()
        .items
}

fn assert_transition(fixture: &PlaybackFixture, id: u64, expected: DownloadTransition) {
    assert!(facts(fixture).iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Download(kind),
            ..
        } if kind == expected
            && item.draft.command_id == Some(CommandId::from_parts(70, id))
    )));
}

fn assert_rejection(fixture: &PlaybackFixture, id: u64, expected: RequestRejectionReason) {
    assert!(facts(fixture).iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Rejected { reason },
        } if reason == expected
            && item.draft.command_id == Some(CommandId::from_parts(70, id))
    )));
}
