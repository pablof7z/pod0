use crate::runtime_agent_modules::tests::{
    next_leased_agent_request, record_leased_agent_observation,
};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;
use pod0_application::{
    ActivityActor, ActivityFact, ActivityOrigin, CommandActivityIdentity, RequestDisposition,
    RequestRejectionReason, user_artifact_migration_command_id,
};

fn projection(facade: &Pod0Facade, scope: MemoryProjectionScope) -> MemoriesProjection {
    let Projection::Memories { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Memories { scope },
            offset: 0,
            max_items: 100,
        })
        .projection
    else {
        panic!("expected memories projection");
    };
    value
}

fn command(id: u64, command: ApplicationCommand) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(80, id),
        cancellation_id: CancellationId::from_parts(81, id),
        expected_revision: None,
        command,
    }
}

fn activate(facade: &Pod0Facade) {
    let digest = ContentDigest::from_bytes([7; 32]);
    let inspection = facade.inspect_legacy_memory_cutover(digest, 24, Vec::new(), None);
    let generation = inspection.source_generation.expect("generation");
    assert_eq!(
        facade
            .stage_legacy_memory_cutover(digest, 24, Vec::new(), None)
            .stage,
        LegacyMemoryCutoverStage::Staged
    );
    assert_eq!(
        facade.verify_legacy_memory_cutover(generation).stage,
        LegacyMemoryCutoverStage::Verified
    );
    let committed = facade.commit_legacy_memory_cutover(generation);
    assert_eq!(
        committed.stage,
        LegacyMemoryCutoverStage::Authoritative,
        "{committed:?}"
    );
}

fn activity(fixture: &PlaybackFixture, id: u64) -> Vec<pod0_application::CommittedActivityFact> {
    let command_id = CommandId::from_parts(80, id);
    let correlation = CommandActivityIdentity::new(command_id).correlation_id();
    pod0_storage::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(correlation, None, 20)
        .unwrap()
        .items
}

fn memory_cutover_activity(
    fixture: &PlaybackFixture,
    generation: u64,
) -> Vec<pod0_application::CommittedActivityFact> {
    let source_id = CommandId::from_parts(0, generation);
    let command_id = user_artifact_migration_command_id("memories", "commit", source_id);
    let correlation = CommandActivityIdentity::new(command_id).correlation_id();
    pod0_storage::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(correlation, None, 20)
        .unwrap()
        .items
}

#[test]
fn memory_commands_are_revision_checked_bounded_and_restart_durable() {
    let fixture = PlaybackFixture::new();
    activate(&fixture.facade);
    fixture.facade.dispatch(command(
        1,
        ApplicationCommand::CreateMemory {
            content: "Prefers concise explanations".to_owned(),
        },
    ));
    let created = projection(&fixture.facade, MemoryProjectionScope::All);
    assert_eq!(created.memories.len(), 1);
    let memory_id = MemoryId::from_bytes(CommandId::from_parts(80, 1).into_bytes());
    assert_eq!(created.memories[0].memory_id, memory_id);
    assert_eq!(created.memories[0].revision, MemoryRevision::INITIAL);
    assert_eq!(activity(&fixture, 1).len(), 2);

    fixture.facade.dispatch(command(
        2,
        ApplicationCommand::UpdateMemory {
            memory_id,
            expected_memory_revision: MemoryRevision::INITIAL,
            content: "Prefers concise, evidence-backed explanations".to_owned(),
        },
    ));
    let updated = projection(&fixture.facade, MemoryProjectionScope::Active);
    assert_eq!(updated.memories[0].revision, MemoryRevision::new(2));
    assert_eq!(activity(&fixture, 2).len(), 2);

    fixture.facade.dispatch(command(
        4,
        ApplicationCommand::UpdateMemory {
            memory_id,
            expected_memory_revision: MemoryRevision::INITIAL,
            content: "Stale mutation".to_owned(),
        },
    ));
    let rejected = activity(&fixture, 4);
    assert_eq!(rejected.len(), 1);
    assert!(matches!(
        rejected[0].draft.fact,
        ActivityFact::RequestDisposition {
            disposition: RequestDisposition::Rejected {
                reason: RequestRejectionReason::RevisionConflict
            }
        }
    ));

    fixture.facade.dispatch(command(
        3,
        ApplicationCommand::SetMemoryDeleted {
            memory_id,
            expected_memory_revision: MemoryRevision::new(2),
            deleted: true,
        },
    ));
    assert!(
        projection(&fixture.facade, MemoryProjectionScope::Active)
            .memories
            .is_empty()
    );
    assert_eq!(activity(&fixture, 3).len(), 2);

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let recovered = projection(&reopened, MemoryProjectionScope::All);
    assert_eq!(recovered.memories.len(), 1);
    assert!(recovered.memories[0].deleted);
    assert_eq!(recovered.memories[0].revision, MemoryRevision::new(3));
}

#[test]
fn legacy_memory_cutover_preserves_compiled_provenance() {
    let fixture = PlaybackFixture::new();
    let memory_id = MemoryId::from_parts(90, 1);
    let memory = LegacyMemoryInput {
        memory_id,
        content: "Listens while commuting".to_owned(),
        created_at: UnixTimestampMilliseconds::new(1_700_000_000_000),
        deleted: false,
    };
    let compiled = LegacyCompiledMemoryInput {
        text: "Usually listens during a commute.".to_owned(),
        compiled_at: UnixTimestampMilliseconds::new(1_700_000_100_000),
        source_memory_ids: vec![memory_id],
    };
    let digest = ContentDigest::from_bytes([9; 32]);
    let inspection = fixture.facade.inspect_legacy_memory_cutover(
        digest,
        64,
        vec![memory.clone()],
        Some(compiled.clone()),
    );
    let generation = inspection.source_generation.unwrap();
    fixture
        .facade
        .stage_legacy_memory_cutover(digest, 64, vec![memory], Some(compiled));
    fixture.facade.verify_legacy_memory_cutover(generation);
    assert!(
        projection(&fixture.facade, MemoryProjectionScope::All)
            .memories
            .is_empty()
    );
    assert!(memory_cutover_activity(&fixture, generation).is_empty());
    fixture.facade.commit_legacy_memory_cutover(generation);
    let activity = memory_cutover_activity(&fixture, generation);
    assert_eq!(activity.len(), 3);
    assert!(activity.iter().all(|fact| {
        fact.draft.actor == ActivityActor::Migration
            && fact.draft.origin == ActivityOrigin::Migration
    }));
    assert!(matches!(
        activity[2].draft.fact,
        ActivityFact::AuthorityCutover { .. }
    ));
    fixture.facade.commit_legacy_memory_cutover(generation);
    assert_eq!(memory_cutover_activity(&fixture, generation), activity);

    let projected = projection(&fixture.facade, MemoryProjectionScope::All);
    assert_eq!(projected.memories.len(), 1);
    assert_eq!(
        projected.compiled.unwrap().source_memory_ids,
        vec![memory_id]
    );
}

#[test]
fn deferred_agent_record_memory_is_not_advertised_or_committed() {
    let fixture = PlaybackFixture::new();
    activate(&fixture.facade);
    let start = command(
        20,
        ApplicationCommand::StartAgentTurn {
            conversation_id: None,
            user_input: "Remember that I prefer primary sources".to_owned(),
            model_reference: "openrouter/test".to_owned(),
        },
    );
    fixture.facade.dispatch(start);
    let model = next_leased_agent_request(&fixture.facade);
    let HostRequest::ExecuteAgentModelTurn { execution } = &model.request.request else {
        panic!("expected model request");
    };
    record_leased_agent_observation(
        &fixture.facade,
        &model,
        HostObservation::AgentModelCompleted {
            turn_id: execution.turn_id,
            model_fence_id: execution.model_fence_id,
            assistant_text: "I'll remember that.".to_owned(),
            proposed_tool_call: Some(AgentModelToolCallObservation {
                provider_call_id: "memory-call".to_owned(),
                tool_name: "record_memory".to_owned(),
                arguments_json: r#"{"text":"Prefers primary sources"}"#.to_owned(),
            }),
            usage: None,
        },
    );
    assert!(fixture.facade.next_leased_host_requests(8).is_empty());
    let memories = projection(&fixture.facade, MemoryProjectionScope::Active);
    assert!(memories.memories.is_empty());
}
