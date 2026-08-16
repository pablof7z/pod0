use pod0_application::{
    TranscriptCapabilityContext, TranscriptCapabilityObservation, TranscriptCapabilityRequest,
    TranscriptProvider, TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin,
    TranscriptWorkflowProjection, TranscriptWorkflowStage,
};
use pod0_domain::{
    ContentDigest, TranscriptArtifactInput, TranscriptArtifactSegmentInput, TranscriptSource,
};
use pod0_storage::ActivityStore;

use crate::runtime_playback_test_support::{PlaybackFixture, library_request};
use crate::*;

#[test]
fn transcript_workflow_commits_indexes_and_survives_relaunch() {
    let fixture = PlaybackFixture::new();
    let command_id = CommandId::from_parts(70, 1);
    fixture.facade.dispatch(CommandEnvelope {
        command_id,
        cancellation_id: CancellationId::from_parts(70, 2),
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
        .find(|leased| {
            matches!(
                leased.request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            )
        })
        .expect("transcript request");
    let request = &leased.request;
    let HostRequest::ExecuteTranscriptCapability {
        capability: TranscriptCapabilityRequest::SubmitProvider { context, .. },
    } = &request.request
    else {
        panic!("expected provider submission");
    };
    let observation = HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(leased.lease.expires_at.value - 1),
        observation: HostObservation::TranscriptCapabilityObserved {
            observation: TranscriptCapabilityObservation::Completed {
                external_operation_id: None,
                provider_status: Some("completed".into()),
                artifact: transcript(context),
            },
        },
    };
    let mut stale_lease = leased.lease;
    stale_lease.fence = stale_lease.fence.saturating_add(1);
    assert!(matches!(
        fixture
            .facade
            .record_leased_host_observation(LeasedHostObservationEnvelope {
                lease: stale_lease,
                observation: observation.clone(),
            }),
        HostObservationReceipt::Rejected {
            reason: HostObservationRejection::StaleWorkflow,
            ..
        }
    ));
    let leased_observation = LeasedHostObservationEnvelope {
        lease: leased.lease,
        observation,
    };
    let receipt = fixture
        .facade
        .record_leased_host_observation(leased_observation.clone());
    assert!(matches!(
        receipt,
        HostObservationReceipt::Persisted { terminal: true, .. }
    ));
    let activity = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 40)
        .unwrap();
    assert!(activity.items.iter().any(|fact| matches!(
        fact.draft.fact,
        pod0_application::ActivityFact::EffectObserved {
            intent_id,
            attempt_id,
            outcome: pod0_application::EffectOutcome::Succeeded,
        } if intent_id == leased.lease.intent_id && attempt_id == leased.lease.attempt_id
    )));
    let fact_count = activity.items.len();
    assert!(matches!(
        fixture
            .facade
            .record_leased_host_observation(leased_observation),
        HostObservationReceipt::Rejected {
            reason: HostObservationRejection::Duplicate,
            ..
        }
    ));
    assert_eq!(
        ActivityStore::open(&fixture.target)
            .unwrap()
            .page_for_episode(fixture.episode_id, None, 40)
            .unwrap()
            .items
            .len(),
        fact_count
    );
    let effect_states: (i64, i64) = rusqlite::Connection::open(&fixture.target)
        .unwrap()
        .query_row(
            "SELECT i.state_code,a.state_code FROM pod0_effect_intents i
             JOIN pod0_effect_attempts a ON a.intent_id=i.intent_id WHERE i.intent_id=?1",
            [leased.lease.intent_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(effect_states, (3, 3));

    crate::runtime_recall_test_support::complete_evidence_embedding_requests(&fixture.facade);
    let completed_activity = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap();
    assert!(completed_activity.items.iter().any(|item| matches!(
        item.draft.fact,
        pod0_application::ActivityFact::InternalCommandAuthorized {
            target: pod0_application::ActivityDomain::RecallKnowledge,
            ..
        }
    )));
    assert!(completed_activity.items.iter().any(|item| matches!(
        item.draft.fact,
        pod0_application::ActivityFact::EffectAuthorized {
            kind: pod0_application::ExternalEffectKind::RecallProvider,
            ..
        }
    )));
    assert!(completed_activity.items.iter().any(|item| matches!(
        item.draft.fact,
        pod0_application::ActivityFact::EffectObserved {
            outcome: pod0_application::EffectOutcome::Succeeded,
            ..
        }
    )));
    let durable_states: (i64, i64) = rusqlite::Connection::open(&fixture.target)
        .unwrap()
        .query_row(
            "SELECT
             (SELECT count(*) FROM pod0_internal_command_intents WHERE state_code=2),
             (SELECT count(*) FROM pod0_effect_intents WHERE effect_kind_code=3 AND state_code=3)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(durable_states, (1, 1));
    let projected = workflow(&fixture.facade, fixture.episode_id);
    assert_eq!(projected.stage, TranscriptWorkflowStage::Succeeded);
    assert!(projected.failure.is_none());
    let selected = transcript_summary(&fixture.facade, fixture.episode_id);
    assert_eq!(selected.source_revision, projected.source_revision);
    assert!(matches!(
        operation(&fixture.facade, command_id).stage,
        OperationStage::Succeeded
    ));

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        workflow(&reopened, fixture.episode_id).stage,
        TranscriptWorkflowStage::Succeeded
    );
    assert_eq!(
        transcript_summary(&reopened, fixture.episode_id).transcript_version_id,
        selected.transcript_version_id
    );
    assert!(
        reopened
            .next_leased_host_requests(u16::MAX)
            .into_iter()
            .all(|request| !matches!(
                request.request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            ))
    );
}

pub(super) fn configuration() -> TranscriptWorkflowConfiguration {
    TranscriptWorkflowConfiguration {
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        local_audio_url: None,
        credential_available: true,
        auto_publisher_enabled: true,
        auto_provider_enabled: true,
    }
}

pub(super) fn transcript(context: &TranscriptCapabilityContext) -> TranscriptArtifactInput {
    TranscriptArtifactInput {
        episode_id: context.episode_id,
        podcast_id: context.podcast_id,
        source_revision: context.source_revision.clone(),
        source: TranscriptSource::AssemblyAi,
        provider: Some("assemblyAI".into()),
        source_payload_digest: ContentDigest::from_bytes([0x71; 32]),
        language: "en-US".into(),
        generated_at: UnixTimestampMilliseconds::new(1_900_000_000_000),
        speakers: Vec::new(),
        segments: vec![TranscriptArtifactSegmentInput {
            text: "Calm by default, alive on demand.".into(),
            start_milliseconds: 0,
            end_milliseconds: 2_000,
            speaker_id: None,
            words: Vec::new(),
        }],
    }
}

pub(super) fn workflow(facade: &Pod0Facade, episode_id: EpisodeId) -> TranscriptWorkflowProjection {
    let Projection::TranscriptWorkflows { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::TranscriptWorkflows {
                episode_id: Some(episode_id),
            },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected transcript workflow projection");
    };
    value.workflows.into_iter().next().expect("workflow")
}

fn transcript_summary(facade: &Pod0Facade, episode_id: EpisodeId) -> TranscriptSummaryProjection {
    let Projection::Transcript { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Transcript {
                episode_id,
                scope: TranscriptProjectionScope::Summary,
            },
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected transcript projection");
    };
    value.summary.expect("selected transcript")
}

fn operation(facade: &Pod0Facade, command_id: CommandId) -> OperationProjection {
    let Projection::Library { value } = facade.snapshot(library_request()).projection else {
        panic!("expected library");
    };
    value
        .operations
        .into_iter()
        .find(|operation| operation.command_id == command_id)
        .expect("operation")
}
