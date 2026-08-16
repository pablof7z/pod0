use pod0_application::{
    ApplicationCommand, CommandEnvelope, HostRequest, TranscriptCredentialCapabilities,
    TranscriptProvider, WorkflowCapabilitySnapshot, WorkflowCapabilitySnapshotInput,
    WorkflowConfigurationInput, WorkflowOpportunity, WorkflowOpportunityReason,
};
use pod0_domain::{CancellationId, CommandId, ContentDigest, UnixTimestampMilliseconds};
use pod0_storage::LibraryStore;

use crate::Pod0Facade;

#[test]
fn workflow_reconcile_consumes_children_and_restart_does_not_duplicate_effects() {
    let fixture = crate::runtime_chapter_workflow_test_support::publisher_fixture();
    let (store, snapshot) = configured_store(&fixture);
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(142, 3),
        cancellation_id: CancellationId::from_parts(142, 3),
        expected_revision: None,
        command: ApplicationCommand::ReconcileWorkflowOpportunity {
            opportunity: WorkflowOpportunity {
                reason: WorkflowOpportunityReason::Foreground,
                observed_at: UnixTimestampMilliseconds::new(1_800_000_000_003),
                capability_snapshot_id: snapshot.snapshot_id,
            },
        },
    });
    assert!(store.pending_internal_commands(100).unwrap().is_empty());
    let requests = fixture.facade.next_leased_host_requests(20);
    assert!(requests.iter().any(|request| matches!(
        request.request.request,
        HostRequest::FetchPublisherChapters { .. }
    )));
    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert!(
        LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .pending_internal_commands(100)
            .unwrap()
            .is_empty()
    );
    assert!(reopened.next_leased_host_requests(20).is_empty());
}

#[test]
fn startup_recovers_children_committed_before_runtime_drain() {
    let fixture = crate::runtime_chapter_workflow_test_support::publisher_fixture();
    let (store, snapshot) = configured_store(&fixture);
    store
        .reconcile_workflow_opportunity(
            CommandId::from_parts(142, 4),
            digest(4),
            opportunity(&snapshot),
        )
        .unwrap();
    assert!(!store.pending_internal_commands(100).unwrap().is_empty());
    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert!(
        LibraryStore::open_authoritative(&fixture.target)
            .unwrap()
            .pending_internal_commands(100)
            .unwrap()
            .is_empty()
    );
    assert!(
        reopened
            .next_leased_host_requests(20)
            .iter()
            .any(|request| matches!(
                request.request.request,
                HostRequest::FetchPublisherChapters { .. }
            ))
    );
}

fn configured_store(
    fixture: &crate::runtime_playback_test_support::PlaybackFixture,
) -> (LibraryStore, WorkflowCapabilitySnapshot) {
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    store
        .import_legacy_workflow_configuration(
            CommandId::from_parts(142, 1),
            digest(1),
            configuration(),
            digest(2),
            1_800_000_000_001,
        )
        .unwrap();
    let snapshot = WorkflowCapabilitySnapshot::from_input(
        WorkflowCapabilitySnapshotInput {
            credentials: TranscriptCredentialCapabilities {
                eleven_labs: false,
                assembly_ai: false,
                open_router: false,
                apple_speech: true,
            },
            local_audio: Vec::new(),
        },
        UnixTimestampMilliseconds::new(1_800_000_000_002),
    )
    .unwrap();
    store
        .observe_workflow_capabilities(CommandId::from_parts(142, 2), digest(3), snapshot.clone())
        .unwrap();
    (store, snapshot)
}

fn opportunity(snapshot: &WorkflowCapabilitySnapshot) -> WorkflowOpportunity {
    WorkflowOpportunity {
        reason: WorkflowOpportunityReason::Foreground,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_000_003),
        capability_snapshot_id: snapshot.snapshot_id,
    }
}

fn configuration() -> WorkflowConfigurationInput {
    WorkflowConfigurationInput {
        transcript_provider: TranscriptProvider::AppleSpeech,
        eleven_labs_model: "scribe_v1".into(),
        assembly_ai_model: "universal-3-pro".into(),
        open_router_model: "openai/whisper-1".into(),
        auto_publisher_transcripts: false,
        auto_provider_transcripts: false,
        chapter_model: "openai/gpt-4o-mini".into(),
    }
}

fn digest(value: u8) -> ContentDigest {
    ContentDigest::from_bytes([value; 32])
}
