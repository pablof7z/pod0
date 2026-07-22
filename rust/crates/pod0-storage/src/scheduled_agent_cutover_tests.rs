use pod0_application::{
    MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES, ScheduledAgentExecutionObservation,
    ScheduledAgentExecutionRequest, ScheduledAgentFailure, ScheduledAgentFailureCode,
    ScheduledAgentOccurrenceState, ScheduledAgentStage, ScheduledTaskDefinition,
    scheduled_attempt_id, scheduled_host_request_id, scheduled_occurrence_id,
    scheduled_prompt_revision,
};
use pod0_domain::{ContentDigest, ScheduledTaskId, StateRevision, UnixTimestampMilliseconds};

use crate::download_store_test_support::DownloadFixture;
use crate::{
    LegacyScheduledAgentCutoverInput, LegacyScheduledAgentOccurrence, LegacyScheduledAgentTask,
    ScheduledAgentCutoverState, ScheduledAgentStore, StorageError,
};

const NOW: i64 = 1_900_000_000_000;

#[test]
fn empty_cutover_is_staged_verified_committed_and_reopen_safe() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let input = input(Vec::new(), Vec::new(), 1);
    let staged = fixture
        .store
        .stage_legacy_scheduled_agent_cutover(input.clone())
        .unwrap();
    let generation = staged.state.source_generation().unwrap();
    assert!(matches!(
        staged.state,
        ScheduledAgentCutoverState::Staged { .. }
    ));
    assert_eq!(
        fixture
            .store
            .stage_legacy_scheduled_agent_cutover(input)
            .unwrap(),
        staged
    );
    assert!(matches!(
        fixture
            .store
            .verify_legacy_scheduled_agent_cutover(generation, time(NOW + 1))
            .unwrap()
            .state,
        ScheduledAgentCutoverState::Verified { .. }
    ));
    let committed = fixture
        .store
        .commit_legacy_scheduled_agent_cutover(generation, time(NOW + 2))
        .unwrap();
    assert!(matches!(
        committed.state,
        ScheduledAgentCutoverState::Authoritative { .. }
    ));
    assert!(ScheduledAgentStore::open_authoritative(&fixture.import.target).is_ok());
    assert_eq!(
        fixture.store.scheduled_agent_cutover_report().unwrap(),
        committed
    );
}

#[test]
fn staged_tasks_terminal_evidence_and_restart_reopen_losslessly() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let task = definition(1, NOW + 86_400_000);
    let pending = occurrence(&task, NOW - 30_000, ScheduledAgentStage::Pending, 0, None);
    let blocked = occurrence(
        &task,
        NOW - 20_000,
        ScheduledAgentStage::Blocked,
        2,
        Some(ScheduledAgentFailure {
            code: ScheduledAgentFailureCode::MissingCredential,
            safe_detail: Some("Connect a provider".to_owned()),
            retryable: true,
        }),
    );
    let mut succeeded = occurrence(&task, NOW - 10_000, ScheduledAgentStage::Succeeded, 3, None);
    let output = "A bounded legacy briefing";
    qualify_completion(&mut succeeded, output);
    let staged = fixture
        .store
        .stage_legacy_scheduled_agent_cutover(input(
            vec![LegacyScheduledAgentTask {
                definition: task.clone(),
            }],
            vec![pending, blocked, succeeded],
            2,
        ))
        .unwrap();
    let generation = staged.state.source_generation().unwrap();
    assert_eq!(staged.task_count, 1);
    assert_eq!(staged.occurrence_count, 3);

    let reopened = crate::LibraryStore::open_authoritative(&fixture.import.target).unwrap();
    assert!(matches!(
        reopened.scheduled_agent_cutover_report().unwrap().state,
        ScheduledAgentCutoverState::Staged { .. }
    ));
    reopened
        .verify_legacy_scheduled_agent_cutover(generation, time(NOW + 1))
        .unwrap();
    reopened
        .commit_legacy_scheduled_agent_cutover(generation, time(NOW + 2))
        .unwrap();
    let store = reopened.scheduled_agent_store().unwrap();
    assert_eq!(store.task_page(0, 10).unwrap().items, [task]);
    let occurrences = store.occurrence_page(None, 0, 10).unwrap().items;
    assert_eq!(occurrences.len(), 3);
    assert!(
        occurrences
            .iter()
            .any(|value| value.stage == ScheduledAgentStage::Succeeded)
    );
    assert!(
        occurrences
            .iter()
            .any(|value| value.stage == ScheduledAgentStage::Blocked)
    );
}

#[test]
fn changed_source_requires_explicit_discard_and_authority_cannot_roll_back() {
    let fixture = DownloadFixture::new_before_download_cutover();
    let first = input(
        vec![LegacyScheduledAgentTask {
            definition: definition(1, NOW),
        }],
        Vec::new(),
        3,
    );
    let staged = fixture
        .store
        .stage_legacy_scheduled_agent_cutover(first)
        .unwrap();
    let generation = staged.state.source_generation().unwrap();
    let changed = input(
        vec![LegacyScheduledAgentTask {
            definition: definition(2, NOW),
        }],
        Vec::new(),
        4,
    );
    assert_eq!(
        fixture
            .store
            .stage_legacy_scheduled_agent_cutover(changed.clone()),
        Err(StorageError::ScheduledAgentWorkflowConflict)
    );
    assert!(
        fixture
            .store
            .discard_staged_legacy_scheduled_agent_cutover(generation)
            .unwrap()
    );
    let replacement = fixture
        .store
        .stage_legacy_scheduled_agent_cutover(changed)
        .unwrap();
    let replacement_generation = replacement.state.source_generation().unwrap();
    fixture
        .store
        .verify_legacy_scheduled_agent_cutover(replacement_generation, time(NOW + 1))
        .unwrap();
    fixture
        .store
        .commit_legacy_scheduled_agent_cutover(replacement_generation, time(NOW + 2))
        .unwrap();
    assert_eq!(
        fixture
            .store
            .discard_staged_legacy_scheduled_agent_cutover(replacement_generation),
        Err(StorageError::CutoverAlreadyAuthoritative)
    );
}

fn input(
    tasks: Vec<LegacyScheduledAgentTask>,
    occurrences: Vec<LegacyScheduledAgentOccurrence>,
    seed: u8,
) -> LegacyScheduledAgentCutoverInput {
    LegacyScheduledAgentCutoverInput {
        backup_digest: ContentDigest::from_bytes([seed; 32]),
        backup_byte_count: 100 + u64::from(seed),
        tasks,
        occurrences,
        observed_at: time(NOW),
    }
}

fn definition(value: u64, next_run_at: i64) -> ScheduledTaskDefinition {
    let prompt = format!("Prepare briefing {value}");
    ScheduledTaskDefinition {
        task_id: ScheduledTaskId::from_parts(800, value),
        label: format!("Briefing {value}"),
        prompt_revision: scheduled_prompt_revision(&prompt).unwrap(),
        prompt,
        model_reference: "openrouter:test/model".to_owned(),
        interval_milliseconds: 86_400_000,
        created_at: time(NOW - 100_000),
        last_run_at: None,
        next_run_at: time(next_run_at),
        revision: StateRevision::new(1),
    }
}

fn occurrence(
    task: &ScheduledTaskDefinition,
    scheduled_for: i64,
    stage: ScheduledAgentStage,
    attempt: u16,
    failure: Option<ScheduledAgentFailure>,
) -> LegacyScheduledAgentOccurrence {
    let occurrence_id = scheduled_occurrence_id(task.task_id, time(scheduled_for));
    let attempt_id = scheduled_attempt_id(occurrence_id, attempt);
    LegacyScheduledAgentOccurrence {
        scheduled_for: time(scheduled_for),
        created_at: time(scheduled_for),
        state: ScheduledAgentOccurrenceState {
            task_id: task.task_id,
            occurrence_id,
            prompt: task.prompt.clone(),
            prompt_revision: task.prompt_revision,
            model_reference: task.model_reference.clone(),
            stage,
            revision: StateRevision::new(1 + u64::from(attempt)),
            attempt,
            attempt_id,
            request_id: attempt_id.map(scheduled_host_request_id),
            provider_operation_id: None,
            not_before: (stage == ScheduledAgentStage::RetryScheduled).then(|| time(NOW + 5_000)),
            artifact_id: None,
            output_digest: None,
            failure,
            updated_at: time(NOW),
        },
        output_excerpt: None,
    }
}

fn qualify_completion(occurrence: &mut LegacyScheduledAgentOccurrence, output: &str) {
    let state = &mut occurrence.state;
    let request = ScheduledAgentExecutionRequest {
        occurrence_id: state.occurrence_id,
        attempt_id: state.attempt_id.unwrap(),
        prompt_revision: state.prompt_revision,
        prompt: state.prompt.clone(),
        model_reference: state.model_reference.clone(),
        context: Vec::new(),
        maximum_output_bytes: MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES as u64,
    };
    let ScheduledAgentExecutionObservation::Completed {
        artifact_id,
        output_digest,
        ..
    } = pod0_application::qualify_scheduled_agent_completion(&request, output).unwrap()
    else {
        unreachable!()
    };
    state.artifact_id = Some(artifact_id);
    state.output_digest = Some(output_digest);
    occurrence.output_excerpt = Some(output.to_owned());
}

fn time(value: i64) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(value)
}
