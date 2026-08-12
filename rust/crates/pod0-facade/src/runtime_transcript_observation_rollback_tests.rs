use pod0_application::{
    ApplicationCommand, CommandEnvelope, DurableTranscriptHostObservation, HostObservation,
    HostObservationEnvelope, HostObservationReceipt, LeasedHostObservationEnvelope,
    TranscriptCapabilityObservation, TranscriptProvider, TranscriptWorkflowConfiguration,
    TranscriptWorkflowOrigin,
};
use pod0_domain::{CancellationId, CommandId, UnixTimestampMilliseconds};
use pod0_storage::{ActivityStore, LibraryStore, StorageError, TranscriptObservationCommitInput};

use crate::runtime_playback_test_support::PlaybackFixture;

#[test]
fn rejected_observation_mutation_rolls_back_staging_facts_and_effect_completion() {
    let fixture = PlaybackFixture::new();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(75, 1),
        cancellation_id: CancellationId::from_parts(75, 2),
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
        .expect("leased transcript request");
    let observation = HostObservationEnvelope {
        request_id: leased.request.request_id,
        cancellation_id: leased.request.cancellation_id,
        observed_request_revision: leased.request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(leased.lease.expires_at.value - 1),
        observation: HostObservation::TranscriptCapabilityObserved {
            observation: TranscriptCapabilityObservation::ProviderAccepted {
                external_operation_id: "provider-operation".into(),
                provider_status: Some("queued".into()),
            },
        },
    };
    let before = fact_count(&fixture);
    let durable = DurableTranscriptHostObservation::from_host(&observation).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(
        store.commit_transcript_observation(TranscriptObservationCommitInput {
            lease: leased.lease,
            observation: durable,
            decision: pod0_application::TranscriptObservationDecision::Completion,
            committed_at: UnixTimestampMilliseconds::new(1_900_000_000_100),
        }),
        Err(StorageError::InvalidActivity)
    );
    assert_eq!(fact_count(&fixture), before);
    let states: (i64, i64, Option<String>) = rusqlite::Connection::open(&fixture.target)
        .unwrap()
        .query_row(
            "SELECT i.state_code,a.state_code,a.observation_json FROM pod0_effect_intents i
             JOIN pod0_effect_attempts a ON a.intent_id=i.intent_id WHERE i.intent_id=?1",
            [leased.lease.intent_id.into_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(states, (2, 1, None));
    assert!(matches!(
        fixture
            .facade
            .record_leased_host_observation(LeasedHostObservationEnvelope {
                lease: leased.lease,
                observation,
            }),
        HostObservationReceipt::Persisted { .. }
    ));
}

fn fact_count(fixture: &PlaybackFixture) -> usize {
    ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 40)
        .unwrap()
        .items
        .len()
}

fn configuration() -> TranscriptWorkflowConfiguration {
    TranscriptWorkflowConfiguration {
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        local_audio_url: None,
        credential_available: true,
        auto_publisher_enabled: true,
        auto_provider_enabled: true,
    }
}
