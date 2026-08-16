use pod0_application::{
    ActivityFact, ChapterTransition, DomainTransitionKind, ExternalEffectKind, HostObservation,
    HostObservationEnvelope, HostObservationReceipt, HostRequest, LeasedHostObservationEnvelope,
    RequestDisposition, RequestRejectionReason,
};
use pod0_storage::{ActivityStore, LibraryStore};

use crate::runtime_chapter_tests::{chapter_input, envelope};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn publisher_chapter_admission_atomically_authorizes_a_durable_leased_effect() {
    let fixture = crate::runtime_chapter_workflow_test_support::publisher_fixture();
    let command = CommandEnvelope {
        command_id: CommandId::from_parts(70, 900),
        cancellation_id: CancellationId::from_parts(71, 900),
        expected_revision: None,
        command: ApplicationCommand::EnsurePublisherChapters {
            episode_id: fixture.episode_id,
        },
    };
    fixture.facade.dispatch(command.clone());
    let committed = activity_for(&fixture);
    assert!(committed.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::DomainTransition {
            kind: DomainTransitionKind::Chapter(ChapterTransition::PublisherWorkflowStateChanged),
            ..
        }
    )));
    assert!(committed.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::EffectAuthorized {
            kind: ExternalEffectKind::PublisherChapterProvider,
            ..
        }
    )));
    let leased = fixture.facade.next_leased_host_requests(1);
    assert_eq!(leased.len(), 1);
    assert!(matches!(
        leased[0].request.request,
        HostRequest::FetchPublisherChapters { .. }
    ));
    fixture.facade.dispatch(command);
    assert_eq!(activity_for(&fixture).len(), committed.len());
}

#[test]
fn publisher_chapter_cancellation_is_logged_and_retires_the_durable_effect() {
    let fixture = crate::runtime_chapter_workflow_test_support::publisher_fixture();
    crate::runtime_chapter_workflow_test_support::dispatch_ensure(
        &fixture.facade,
        fixture.episode_id,
        910,
    );
    let original = fixture.facade.next_leased_host_requests(1).pop().unwrap();
    let workflow = LibraryStore::open_authoritative(&fixture.target)
        .unwrap()
        .publisher_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(70, 911),
        cancellation_id: CancellationId::from_parts(71, 911),
        expected_revision: None,
        command: ApplicationCommand::CancelPublisherChapters {
            episode_id: fixture.episode_id,
            expected_workflow_revision: workflow.workflow_revision,
        },
    });
    let late = crate::runtime_chapter_workflow_test_support::response(
        &original.request,
        1,
        200,
        crate::runtime_chapter_workflow_test_support::valid_document(),
    );
    assert!(matches!(
        fixture.facade.record_leased_host_observation(
            pod0_application::LeasedHostObservationEnvelope {
                lease: original.lease,
                observation: late,
            },
        ),
        pod0_application::HostObservationReceipt::Rejected {
            reason: pod0_application::HostObservationRejection::StaleWorkflow,
            ..
        }
    ));
    assert_restart_cancellation(&fixture, original.request.request_id);
    assert!(activity_for(&fixture).iter().any(|item| {
        item.draft.command_id == Some(CommandId::from_parts(70, 911))
            && matches!(
                item.draft.fact,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Chapter(
                        ChapterTransition::PublisherWorkflowStateChanged
                    ),
                    ..
                }
            )
    }));
}

#[test]
fn publisher_observation_atomically_records_effect_state_and_artifact_transitions() {
    let fixture = crate::runtime_chapter_workflow_test_support::publisher_fixture();
    crate::runtime_chapter_workflow_test_support::dispatch_ensure(
        &fixture.facade,
        fixture.episode_id,
        899,
    );
    let leased = fixture.facade.next_leased_host_requests(1).pop().unwrap();
    let observation = crate::runtime_chapter_workflow_test_support::response(
        &leased.request,
        1,
        200,
        crate::runtime_chapter_workflow_test_support::valid_document(),
    );
    let receipt = fixture.facade.record_leased_host_observation(
        pod0_application::LeasedHostObservationEnvelope {
            lease: leased.lease,
            observation,
        },
    );
    assert!(matches!(
        receipt,
        pod0_application::HostObservationReceipt::Persisted { terminal: true, .. }
    ));
    let committed = activity_for(&fixture);
    assert!(committed.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::EffectObserved {
            outcome: pod0_application::EffectOutcome::Succeeded,
            ..
        }
    )));
    for expected in [
        ChapterTransition::PublisherWorkflowStateChanged,
        ChapterTransition::ArtifactAdopted,
        ChapterTransition::SelectionChanged,
    ] {
        assert!(committed.iter().any(|item| matches!(
            item.draft.fact,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Chapter(kind),
                ..
            } if kind == expected
        )));
    }
    assert!(fixture.facade.next_leased_host_requests(1).is_empty());
}

#[test]
fn model_cancellation_is_logged_and_retires_the_model_effect() {
    let fixture = PlaybackFixture::new_with_transcript(true);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(72, 922),
        cancellation_id: CancellationId::from_parts(73, 922),
        expected_revision: None,
        command: ApplicationCommand::EnsureModelChapters {
            episode_id: fixture.episode_id,
            configured_model: "ollama:llama3.2".into(),
        },
    });
    let original = fixture
        .facade
        .next_leased_host_requests(8)
        .into_iter()
        .find(|request| {
            matches!(
                request.request.request,
                HostRequest::ExecuteChapterModel { .. }
            )
        })
        .expect("claimed model chapter request");
    let workflow = LibraryStore::open_authoritative(&fixture.target)
        .unwrap()
        .model_chapter_workflow(fixture.episode_id)
        .unwrap()
        .unwrap();
    let cancel_id = CommandId::from_parts(72, 923);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: cancel_id,
        cancellation_id: CancellationId::from_parts(73, 923),
        expected_revision: None,
        command: ApplicationCommand::CancelModelChapters {
            episode_id: fixture.episode_id,
            expected_workflow_revision: workflow.workflow_revision,
        },
    });
    let late = fixture
        .facade
        .record_leased_host_observation(LeasedHostObservationEnvelope {
            lease: original.lease,
            observation: HostObservationEnvelope {
                request_id: original.request.request_id,
                cancellation_id: original.request.cancellation_id,
                observed_request_revision: original.request.issued_revision,
                sequence_number: 1,
                observed_at: UnixTimestampMilliseconds::new(original.lease.expires_at.value - 1),
                observation: HostObservation::Cancelled,
            },
        });
    assert!(matches!(late, HostObservationReceipt::Rejected { .. }));
    assert_restart_cancellation(&fixture, original.request.request_id);
    assert!(activity_for(&fixture).iter().any(|item| {
        item.draft.command_id == Some(cancel_id)
            && matches!(
                item.draft.fact,
                ActivityFact::DomainTransition {
                    kind: DomainTransitionKind::Chapter(
                        ChapterTransition::ModelWorkflowStateChanged
                    ),
                    ..
                }
            )
    }));
}

fn assert_restart_cancellation(fixture: &PlaybackFixture, target_request_id: HostRequestId) {
    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let leased = reopened
        .next_leased_host_requests(8)
        .into_iter()
        .find(|request| {
            matches!(
                request.request.request,
                HostRequest::CancelAuthorizedEffect { target_request_id: target }
                    if target == target_request_id
            )
        })
        .expect("persisted cancellation lease survives restart");
    assert_eq!(leased.request.deadline_at, None);
}

#[test]
fn chapter_ingestion_activity_is_atomic_complete_and_replay_safe() {
    let fixture = PlaybackFixture::new_with_chapters();
    let accepted = envelope(40, chapter_input(&fixture, "Fresh chapter"), 1);
    fixture.facade.dispatch(accepted.clone());
    let accepted_activity = activity_for(&fixture);
    for expected in [
        ChapterTransition::ArtifactAdopted,
        ChapterTransition::SelectionChanged,
    ] {
        assert!(accepted_activity.iter().any(|item| matches!(
            item.draft.fact,
            ActivityFact::DomainTransition {
                kind: DomainTransitionKind::Chapter(kind),
                ..
            } if kind == expected
        )));
    }
    let accepted_count = accepted_activity.len();
    fixture.facade.dispatch(accepted);
    assert_eq!(activity_for(&fixture).len(), accepted_count);
    fixture
        .facade
        .dispatch(envelope(41, chapter_input(&fixture, "Stale chapter"), 1));
    let mut invalid = chapter_input(&fixture, "Invalid chapter");
    invalid.chapters.clear();
    fixture.facade.dispatch(envelope(42, invalid, 2));
    let rejected = activity_for(&fixture);
    assert!(has_rejection(
        &rejected,
        41,
        RequestRejectionReason::RevisionConflict
    ));
    assert!(has_rejection(
        &rejected,
        42,
        RequestRejectionReason::Invalid
    ));
}

fn activity_for(fixture: &PlaybackFixture) -> Vec<pod0_application::CommittedActivityFact> {
    ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap()
        .items
}

fn has_rejection(
    activity: &[pod0_application::CommittedActivityFact],
    command_suffix: u64,
    expected: RequestRejectionReason,
) -> bool {
    activity.iter().any(|item| {
        matches!(
            item.draft.fact,
            ActivityFact::RequestDisposition {
                disposition: RequestDisposition::Rejected { reason },
            } if reason == expected
                && item.draft.command_id == Some(CommandId::from_parts(30, command_suffix))
        )
    })
}
