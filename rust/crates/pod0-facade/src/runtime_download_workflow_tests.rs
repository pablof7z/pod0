use std::sync::Arc;

use pod0_application::{ActivityDomain, ActivityFact, ActivityOrigin, DomainTransitionKind};
use pod0_storage::{ActivityStore, LibraryStore};

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[path = "runtime_download_workflow_test_support.rs"]
mod support;
pub(super) use support::*;

#[test]
fn staged_host_file_becomes_durable_episode_state_and_projection() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    observe_wifi(&fixture.facade, 1);
    request_download(&fixture, 2);
    let request = fixture.facade.next_leased_host_requests(20).pop().unwrap();
    let media = fixture.target.parent().unwrap().join("native-staged.media");
    let bytes = b"facade durable media";
    std::fs::write(&media, bytes).unwrap();

    let receipt = fixture
        .facade
        .record_leased_host_observation(leased_staged_observation(
            &request,
            1,
            media.to_string_lossy().into_owned(),
            bytes.len() as u64,
        ));
    assert_eq!(
        receipt,
        HostObservationReceipt::Persisted {
            request_id: request.request.request_id,
            terminal: true,
        }
    );
    assert_eq!(
        workflows(&fixture.facade, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::Succeeded
    );
    let Projection::EpisodeDetail { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::EpisodeDetail {
                episode_id: fixture.episode_id,
            },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected episode projection")
    };
    assert!(matches!(
        value.episode.unwrap().download,
        DownloadArtifactStatus::Available { byte_count, .. } if byte_count == bytes.len() as u64
    ));
    let activity = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 200)
        .unwrap()
        .items;
    let authorization = activity
        .iter()
        .find(|item| {
            matches!(
                item.draft.fact,
                ActivityFact::InternalCommandAuthorized {
                    target: ActivityDomain::Download,
                    ..
                }
            )
        })
        .expect("staged observation authorizes durable Rust finalization");
    assert!(activity.iter().any(|item| {
        item.draft.caused_by_activity_id == Some(authorization.draft.activity_id)
            && item.draft.origin == ActivityOrigin::InternalCommand
            && matches!(
                item.draft.fact,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Download(_),
                    ..
                }
            )
    }));
    assert!(
        LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .pending_download_finalization_commands(20)
            .unwrap()
            .is_empty()
    );

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        workflows(&reopened, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::Succeeded
    );
    assert!(reopened.next_leased_host_requests(20).is_empty());
}

#[test]
fn restart_consumes_durable_download_finalization_command() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    observe_wifi(&fixture.facade, 31);
    request_download(&fixture, 32);
    let request = fixture.facade.next_leased_host_requests(20).pop().unwrap();
    let media = fixture
        .target
        .parent()
        .unwrap()
        .join("restart-finalize.media");
    let bytes = b"restart finalization media";
    std::fs::write(&media, bytes).unwrap();
    let leased = leased_staged_observation(
        &request,
        1,
        media.to_string_lossy().into_owned(),
        bytes.len() as u64,
    );
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    store
        .commit_download_observation(pod0_storage::DownloadObservationCommitInput {
            lease: leased.lease,
            observation: leased.observation,
            action: pod0_storage::DownloadLeasedObservationAction::Staged {
                staged_file_path: media.to_string_lossy().into_owned(),
                claimed_byte_count: bytes.len() as u64,
            },
            committed_at: UnixTimestampMilliseconds::new(1_800_000_000_100),
        })
        .unwrap();
    assert_eq!(
        store
            .download_workflow(fixture.episode_id)
            .unwrap()
            .unwrap()
            .stage,
        pod0_storage::StoredDownloadStage::Transferring
    );
    assert_eq!(
        store
            .pending_download_finalization_commands(20)
            .unwrap()
            .len(),
        1
    );
    let target = fixture.target.clone();
    let episode_id = fixture.episode_id;
    drop(fixture.facade);

    let reopened = Pod0Facade::open_with_clock(
        target.to_string_lossy().into_owned(),
        Arc::new(FixedClock(1_800_000_000_200)),
    );
    assert_eq!(
        workflows(&reopened, episode_id).workflows[0].stage,
        DownloadWorkflowStage::Succeeded
    );
    assert!(
        LibraryStore::open_authoritative(&target)
            .unwrap()
            .pending_download_finalization_commands(20)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn cancellation_fences_late_start_completion_and_emits_durable_cancel_request() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    observe_wifi(&fixture.facade, 1);
    request_download(&fixture, 2);
    let start = fixture.facade.next_leased_host_requests(20).pop().unwrap();
    let projection = workflows(&fixture.facade, fixture.episode_id)
        .workflows
        .remove(0);
    dispatch(
        &fixture.facade,
        3,
        ApplicationCommand::CancelEpisodeDownload {
            episode_id: fixture.episode_id,
            expected_workflow_revision: projection.workflow_revision,
        },
    );
    let cancel = fixture.facade.next_leased_host_requests(20).pop().unwrap();
    assert!(matches!(
        cancel.request.request,
        HostRequest::CancelEpisodeDownload { .. }
    ));

    let media = fixture.target.parent().unwrap().join("late.media");
    std::fs::write(&media, b"late").unwrap();
    assert!(matches!(
        fixture
            .facade
            .record_leased_host_observation(leased_staged_observation(
                &start,
                1,
                media.to_string_lossy().into_owned(),
                4,
            )),
        HostObservationReceipt::Rejected {
            reason: HostObservationRejection::UnknownRequest,
            ..
        } | HostObservationReceipt::Rejected {
            reason: HostObservationRejection::Cancelled,
            ..
        } | HostObservationReceipt::Rejected {
            reason: HostObservationRejection::StaleWorkflow,
            ..
        }
    ));
    assert_eq!(
        workflows(&fixture.facade, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::Cancelled
    );

    let HostRequest::CancelEpisodeDownload {
        episode_id,
        intent_id,
        attempt_id,
        ..
    } = cancel.request.request
    else {
        panic!("expected cancellation")
    };
    assert_eq!(
        fixture
            .facade
            .record_leased_host_observation(leased_observation(
                &cancel,
                1,
                HostObservation::DownloadCancelled {
                    episode_id,
                    intent_id,
                    attempt_id
                },
            )),
        HostObservationReceipt::Persisted {
            request_id: cancel.request.request_id,
            terminal: true
        }
    );
}

#[test]
fn retry_request_is_not_emitted_until_kernel_owned_deadline() {
    let fixture = PlaybackFixture::new();
    fixture
        .facade
        .state()
        .set_clock(Arc::new(FixedClock(1_800_000_000_000)));
    observe_wifi(&fixture.facade, 1);
    request_download(&fixture, 2);
    let request = fixture.facade.next_leased_host_requests(20).pop().unwrap();
    let receipt = fixture
        .facade
        .record_leased_host_observation(leased_observation(
            &request,
            1,
            HostObservation::Failed {
                code: HostFailureCode::Offline,
                safe_detail: None,
            },
        ));
    assert!(matches!(
        receipt,
        HostObservationReceipt::Persisted { terminal: true, .. }
    ));
    assert!(fixture.facade.next_leased_host_requests(20).is_empty());
    assert_eq!(
        workflows(&fixture.facade, fixture.episode_id).workflows[0].stage,
        DownloadWorkflowStage::RetryScheduled
    );

    fixture.facade.state().set_clock(Arc::new(FixedClock(
        pod0_application::download_retry_not_before(request.lease.expires_at).value,
    )));
    let retry = fixture.facade.next_leased_host_requests(20).pop().unwrap();
    assert_ne!(retry.request.request_id, request.request.request_id);
    assert!(matches!(
        retry.request.request,
        HostRequest::StartEpisodeDownload { .. }
    ));
}
