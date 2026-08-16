use pod0_application::{
    ApplicationCommand, CommandEnvelope, HostObservation, HostObservationEnvelope,
    HostObservationReceipt, HostObservationRejection, HostRequest, LeasedHostObservationEnvelope,
    Projection, ProjectionRequest, ProjectionScope, TranscriptProvider,
    TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin, TranscriptWorkflowStage,
};
use pod0_domain::{CancellationId, CommandId, UnixTimestampMilliseconds};
use pod0_storage::ActivityStore;

use crate::Pod0Facade;
use crate::runtime_playback_test_support::PlaybackFixture;

#[test]
fn cancellation_atomically_records_the_transition_and_retires_a_claimed_effect() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(74, 1),
        cancellation_id: CancellationId::from_parts(74, 2),
        expected_revision: None,
        command: ApplicationCommand::EnsureTranscriptWorkflow {
            episode_id: fixture.episode_id,
            origin: TranscriptWorkflowOrigin::User,
            configuration: configuration(),
        },
    });
    let leased = fixture
        .facade
        .next_leased_host_requests(u16::MAX)
        .into_iter()
        .next()
        .expect("claimed transcript submission");
    let revision = workflow_stage_and_revision(&fixture).1;
    let cancel = CommandEnvelope {
        command_id: CommandId::from_parts(74, 3),
        cancellation_id: CancellationId::from_parts(74, 4),
        expected_revision: None,
        command: ApplicationCommand::CancelTranscriptWorkflow {
            episode_id: fixture.episode_id,
            expected_workflow_revision: revision,
        },
    };
    fixture.facade.dispatch(cancel.clone());

    assert_eq!(
        workflow_stage_and_revision(&fixture).0,
        TranscriptWorkflowStage::Cancelled
    );
    let facts = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 20)
        .unwrap()
        .items;
    assert!(facts.iter().any(|fact| matches!(
        fact.draft.fact,
        pod0_application::ActivityFact::DomainTransition {
            kind: pod0_application::DomainTransitionKind::Transcript(
                pod0_application::TranscriptTransition::Cancelled
            ),
            ..
        }
    )));
    let states: (i64, i64) = rusqlite::Connection::open(&fixture.target)
        .unwrap()
        .query_row(
            "SELECT i.state_code,a.state_code FROM pod0_effect_intents i
             JOIN pod0_effect_attempts a ON a.intent_id=i.intent_id WHERE i.intent_id=?1",
            [leased.lease.intent_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(states, (4, 4));

    let late = fixture
        .facade
        .record_leased_host_observation(LeasedHostObservationEnvelope {
            lease: leased.lease,
            observation: HostObservationEnvelope {
                request_id: leased.request.request_id,
                cancellation_id: leased.request.cancellation_id,
                observed_request_revision: leased.request.issued_revision,
                sequence_number: 1,
                observed_at: UnixTimestampMilliseconds::new(leased.lease.expires_at.value - 1),
                observation: HostObservation::Cancelled,
            },
        });
    assert!(
        matches!(
            late,
            HostObservationReceipt::Rejected {
                reason: HostObservationRejection::StaleWorkflow
                    | HostObservationRejection::UnknownRequest
                    | HostObservationRejection::Cancelled
                    | HostObservationRejection::MismatchedPayload,
                ..
            }
        ),
        "late original observation must fail closed: {late:?}"
    );

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert!(
        reopened
            .next_leased_host_requests(8)
            .into_iter()
            .any(|request| matches!(
                request.request.request,
                HostRequest::CancelAuthorizedEffect { target_request_id }
                    if target_request_id == leased.request.request_id
            ))
    );

    reopened.dispatch(cancel);
    assert_eq!(
        ActivityStore::open(&fixture.target)
            .unwrap()
            .page_for_episode(fixture.episode_id, None, 20)
            .unwrap()
            .items
            .len(),
        facts.len()
    );
}

fn workflow_stage_and_revision(
    fixture: &PlaybackFixture,
) -> (TranscriptWorkflowStage, pod0_domain::StateRevision) {
    let Projection::TranscriptWorkflows { value } = fixture
        .facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::TranscriptWorkflows {
                episode_id: Some(fixture.episode_id),
            },
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected transcript workflow projection")
    };
    let workflow = value.workflows.into_iter().next().unwrap();
    (workflow.stage, workflow.workflow_revision)
}

fn configuration() -> TranscriptWorkflowConfiguration {
    TranscriptWorkflowConfiguration {
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        local_audio_url: None,
        credential_available: true,
        auto_publisher_enabled: false,
        auto_provider_enabled: true,
    }
}
