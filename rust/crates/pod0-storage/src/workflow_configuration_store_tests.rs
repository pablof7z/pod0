use pod0_application::{
    LocalAudioCapability, TranscriptCredentialCapabilities, TranscriptProvider,
    WorkflowCapabilitySnapshot, WorkflowCapabilitySnapshotInput, WorkflowConfigurationInput,
    WorkflowConfigurationOrigin, WorkflowOpportunity, WorkflowOpportunityReason,
};
use pod0_domain::{CommandId, ContentDigest, StateRevision, UnixTimestampMilliseconds};
use rusqlite::Connection;

use crate::library_store_tests::imported_fixture;
use crate::{LibraryStore, commit_listening_cutover};
use pod0_application::RequestDisposition;

#[test]
fn typed_import_is_atomic_replayable_and_survives_restart() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(store.workflow_configuration().unwrap(), None);
    let command = CommandId::from_parts(41, 1);
    let fingerprint = digest(1);
    let input = configuration();
    let first = store
        .import_legacy_workflow_configuration(
            command, fingerprint, input.clone(), digest(2), 1_800_000_000_001,
        )
        .unwrap();
    assert!(first.changed && first.imported);
    assert_eq!(first.receipt.disposition, RequestDisposition::Accepted);
    let replay = store
        .import_legacy_workflow_configuration(
            command, fingerprint, input, digest(2), 1_800_000_000_001,
        )
        .unwrap();
    assert!(replay.receipt.replayed);
    let reopened = LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert_eq!(
        reopened.workflow_configuration().unwrap().unwrap().origin,
        WorkflowConfigurationOrigin::LegacySwiftImport
    );
}

#[test]
fn stale_setting_revision_is_durably_rejected_without_mutation() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let imported = store
        .import_legacy_workflow_configuration(
            CommandId::from_parts(41, 3), digest(3), configuration(), digest(4),
            1_800_000_000_001,
        )
        .unwrap()
        .configuration
        .unwrap();
    let mut changed = configuration();
    changed.chapter_model = "openai/gpt-5".into();
    let stale = store
        .set_workflow_configuration(
            CommandId::from_parts(41, 4), digest(5), StateRevision::INITIAL, changed,
            1_800_000_000_002,
        )
        .unwrap();
    assert!(matches!(
        stale.receipt.disposition,
        RequestDisposition::Rejected { .. }
    ));
    assert_eq!(store.workflow_configuration().unwrap(), Some(imported));
}

#[test]
fn capability_observation_requires_authority_and_persists_no_credential_secret() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    let snapshot = WorkflowCapabilitySnapshot::from_input(
        WorkflowCapabilitySnapshotInput {
            credentials: TranscriptCredentialCapabilities {
                eleven_labs: true,
                assembly_ai: false,
                open_router: false,
                apple_speech: true,
            },
            local_audio: vec![LocalAudioCapability {
                episode_id: pod0_domain::EpisodeId::from_parts(41, 1),
                local_audio_url: "file:///private/audio.m4a".into(),
            }],
        },
        UnixTimestampMilliseconds::new(1_800_000_000_002),
    )
    .unwrap();
    let rejected = store
        .observe_workflow_capabilities(CommandId::from_parts(41, 5), digest(6), snapshot.clone())
        .unwrap();
    assert!(matches!(rejected.receipt.disposition, RequestDisposition::Rejected { .. }));
    store
        .import_legacy_workflow_configuration(
            CommandId::from_parts(41, 6), digest(7), configuration(), digest(8),
            1_800_000_000_003,
        )
        .unwrap();
    let accepted = store
        .observe_workflow_capabilities(CommandId::from_parts(41, 7), digest(9), snapshot)
        .unwrap();
    assert!(accepted.changed);
    let connection = Connection::open(&fixture.target).unwrap();
    let persisted: String = connection
        .query_row("SELECT snapshot_json FROM pod0_workflow_capability_snapshot", [], |row| row.get(0))
        .unwrap();
    assert!(!persisted.contains("api-key") && !persisted.contains("token"));
    let journal: String = connection
        .query_row(
            "SELECT group_concat(payload_json,'|') FROM pod0_activity_facts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!journal.contains("/private/audio.m4a") && !journal.contains("api-key"));
}

#[test]
fn reconciliation_atomically_authorizes_children_and_replays_exactly() {
    let fixture = imported_fixture();
    commit_listening_cutover(&fixture.target, 1_800_000_000_000).unwrap();
    let store = LibraryStore::open_authoritative(&fixture.target).unwrap();
    store
        .import_legacy_workflow_configuration(
            CommandId::from_parts(41, 8), digest(10), configuration(), digest(11),
            1_800_000_000_001,
        )
        .unwrap();
    let snapshot = WorkflowCapabilitySnapshot::from_input(
        WorkflowCapabilitySnapshotInput {
            credentials: TranscriptCredentialCapabilities {
                eleven_labs: false,
                assembly_ai: true,
                open_router: false,
                apple_speech: true,
            },
            local_audio: Vec::new(),
        },
        UnixTimestampMilliseconds::new(1_800_000_000_002),
    )
    .unwrap();
    store
        .observe_workflow_capabilities(
            CommandId::from_parts(41, 9), digest(12), snapshot.clone(),
        )
        .unwrap();
    let opportunity = WorkflowOpportunity {
        reason: WorkflowOpportunityReason::Foreground,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_000_003),
        capability_snapshot_id: snapshot.snapshot_id,
    };
    let first = store
        .reconcile_workflow_opportunity(CommandId::from_parts(41, 10), digest(13), opportunity)
        .unwrap();
    assert_eq!(first.receipt.disposition, RequestDisposition::Accepted);
    assert!(first.authorized_command_count > 0);
    let replay = LibraryStore::open_authoritative(&fixture.target)
        .unwrap()
        .reconcile_workflow_opportunity(CommandId::from_parts(41, 10), digest(13), opportunity)
        .unwrap();
    assert!(replay.receipt.replayed);
    assert_eq!(replay.receipt.transaction_id, first.receipt.transaction_id);
}

fn configuration() -> WorkflowConfigurationInput {
    WorkflowConfigurationInput {
        transcript_provider: TranscriptProvider::AssemblyAi,
        eleven_labs_model: "scribe_v1".into(),
        assembly_ai_model: "universal-3-pro".into(),
        open_router_model: "openai/whisper-1".into(),
        auto_publisher_transcripts: true,
        auto_provider_transcripts: true,
        chapter_model: "openai/gpt-4o-mini".into(),
    }
}

fn digest(value: u8) -> ContentDigest { ContentDigest::from_bytes([value; 32]) }
