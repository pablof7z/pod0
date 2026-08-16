use crate::runtime_recall_test_support::{
    RecallFixture, commit_test_evidence, complete_evidence_embedding_requests, evidence_input,
    evidence_policy, record,
};
use crate::*;
use pod0_application::DurableRecallIndexCutoverHostObservation;

#[test]
fn cutover_waits_for_rust_index_then_commits_only_after_native_deletion_receipt() {
    let fixture = RecallFixture::new(false);
    commit_test_evidence(
        &fixture.base,
        CommandId::from_parts(90, 1),
        &fixture.artifact,
        100,
    );

    let premature = cutover_command(4);
    fixture.base.facade.dispatch(premature.clone());
    assert!(fixture.base.facade.next_leased_host_requests(1).is_empty());
    assert_eq!(
        operation(&fixture.base.facade, premature.command_id).stage,
        OperationStage::Failed
    );
    assert!(!cutover_marker(&fixture));

    fixture.base.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(90, 5),
        cancellation_id: CancellationId::from_parts(91, 5),
        expected_revision: None,
        command: ApplicationCommand::RebuildTranscriptEvidence {
            input: evidence_input(&fixture.base),
            policy: evidence_policy(),
        },
    });
    complete_evidence_embedding_requests(&fixture.base.facade);

    let command = cutover_command(6);
    fixture.base.facade.dispatch(command.clone());
    let request = fixture
        .base
        .facade
        .next_leased_host_requests(1)
        .pop()
        .unwrap();
    assert_eq!(
        request.request.request,
        HostRequest::RemoveLegacyRecallIndexArtifacts
    );
    assert_eq!(
        operation(&fixture.base.facade, command.command_id).stage,
        OperationStage::Running
    );
    assert!(!cutover_marker(&fixture));

    record(
        &fixture.base.facade,
        &request,
        HostObservation::LegacyRecallIndexArtifactsRemoved {
            removed_file_count: 2,
        },
    );
    let completed = operation(&fixture.base.facade, command.command_id);
    assert_eq!(completed.stage, OperationStage::Succeeded);
    assert_eq!(
        completed.result,
        Some(OperationResult::RecallIndexCutoverCommitted {
            schema_version: pod0_recall_index::RECALL_INDEX_SCHEMA_VERSION,
            removed_legacy_file_count: 2,
        })
    );
    assert!(cutover_marker(&fixture));

    let replay = cutover_command(7);
    fixture.base.facade.dispatch(replay.clone());
    assert!(fixture.base.facade.next_leased_host_requests(1).is_empty());
    assert_eq!(
        operation(&fixture.base.facade, replay.command_id).result,
        Some(OperationResult::RecallIndexCutoverCommitted {
            schema_version: pod0_recall_index::RECALL_INDEX_SCHEMA_VERSION,
            removed_legacy_file_count: 0,
        })
    );
}

#[test]
fn failed_native_deletion_never_commits_cutover_marker() {
    let fixture = RecallFixture::new(true);
    let command = cutover_command(8);
    fixture.base.facade.dispatch(command.clone());
    let request = fixture
        .base
        .facade
        .next_leased_host_requests(1)
        .pop()
        .unwrap();

    record(
        &fixture.base.facade,
        &request,
        HostObservation::Failed {
            code: HostFailureCode::PlatformFailure,
            safe_detail: None,
        },
    );

    let failed = operation(&fixture.base.facade, command.command_id);
    assert_eq!(failed.stage, OperationStage::Failed);
    assert_eq!(
        failed.failure.map(|failure| failure.code),
        Some(CoreFailureCode::HostUnavailable)
    );
    assert!(!cutover_marker(&fixture));
}

#[test]
fn restart_finishes_a_durably_observed_cutover_without_reissuing_deletion() {
    let fixture = RecallFixture::new(true);
    let command = cutover_command(9);
    fixture.base.facade.dispatch(command.clone());
    let request = fixture
        .base
        .facade
        .next_leased_host_requests(1)
        .pop()
        .unwrap();
    let host = HostObservationEnvelope {
        request_id: request.request.request_id,
        cancellation_id: request.request.cancellation_id,
        observed_request_revision: request.request.issued_revision,
        sequence_number: 0,
        observed_at: request.lease.expires_at,
        observation: HostObservation::LegacyRecallIndexArtifactsRemoved {
            removed_file_count: 2,
        },
    };
    let durable = DurableRecallIndexCutoverHostObservation::from_host(&host).unwrap();
    let store = pod0_storage::LibraryStore::open_authoritative(&fixture.base.target).unwrap();
    let (observed, replayed) = store
        .commit_recall_index_cutover_observation(request.lease, durable, request.lease.expires_at)
        .unwrap();
    assert!(!replayed);
    assert!(matches!(
        observed.stage,
        pod0_storage::RecallIndexCutoverStage::HostObserved {
            removed_file_count: 2
        }
    ));
    assert!(!cutover_marker(&fixture));

    let reopened = Pod0Facade::open(fixture.base.target.to_string_lossy().into_owned()).unwrap();
    assert!(reopened.next_leased_host_requests(1).is_empty());
    assert!(cutover_marker(&fixture));
    assert!(matches!(
        store
            .recall_index_cutover_workflow()
            .unwrap()
            .unwrap()
            .stage,
        pod0_storage::RecallIndexCutoverStage::Committed {
            removed_file_count: 2
        }
    ));
}

fn cutover_command(id: u64) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(90, id),
        cancellation_id: CancellationId::from_parts(91, id),
        expected_revision: None,
        command: ApplicationCommand::CommitRecallIndexCutover,
    }
}

fn operation(facade: &Pod0Facade, command_id: CommandId) -> OperationProjection {
    let Projection::Library { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected library projection");
    };
    value
        .operations
        .into_iter()
        .find(|operation| operation.command_id == command_id)
        .expect("operation should be projected")
}

fn cutover_marker(fixture: &RecallFixture) -> bool {
    let path = pod0_recall_index::recall_index_path_for_core_store(&fixture.base.target);
    pod0_recall_index::RecallIndex::open(&path, pod0_recall_index::RECALL_INDEX_DIMENSIONS)
        .unwrap()
        .legacy_cutover_is_committed()
        .unwrap()
}
