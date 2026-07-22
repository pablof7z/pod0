use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn typed_cutover_maps_legacy_input_and_activates_runtime_projection() {
    let fixture = PlaybackFixture::new();
    let task_id = ScheduledTaskId::from_parts(88, 1);
    let task = LegacyScheduledAgentTaskInput {
        task_id,
        label: "Daily briefing".to_owned(),
        prompt: "Summarize saved episodes".to_owned(),
        model_reference: "openrouter:test/model".to_owned(),
        interval_milliseconds: 86_400_000,
        created_at: UnixTimestampMilliseconds::new(1_800_000_000_000),
        last_run_at: None,
        next_run_at: UnixTimestampMilliseconds::new(1_900_000_086_400),
    };
    let occurrence = LegacyScheduledAgentOccurrenceInput {
        task_id,
        scheduled_for: UnixTimestampMilliseconds::new(1_900_000_000_000),
        created_at: UnixTimestampMilliseconds::new(1_900_000_000_000),
        prompt: task.prompt.clone(),
        model_reference: task.model_reference.clone(),
        updated_at: UnixTimestampMilliseconds::new(1_900_000_000_100),
        disposition: LegacyScheduledAgentOccurrenceDisposition::Succeeded {
            attempt: 1,
            output_excerpt: "The durable briefing".to_owned(),
        },
    };
    let backup_digest = ContentDigest::from_bytes([7; 32]);
    let inspected = fixture.facade.inspect_legacy_scheduled_agent_cutover(
        backup_digest,
        512,
        vec![task.clone()],
        vec![occurrence.clone()],
    );
    assert_eq!(
        inspected.stage,
        LegacyScheduledAgentCutoverStage::NotStarted
    );
    let generation = inspected.source_generation.unwrap();
    let staged = fixture.facade.stage_legacy_scheduled_agent_cutover(
        backup_digest,
        512,
        vec![task],
        vec![occurrence],
    );
    assert_eq!(staged.stage, LegacyScheduledAgentCutoverStage::Staged);
    assert_eq!(staged.source_generation, Some(generation));
    assert_eq!(
        fixture
            .facade
            .verify_legacy_scheduled_agent_cutover(generation)
            .stage,
        LegacyScheduledAgentCutoverStage::Verified
    );
    assert_eq!(
        fixture
            .facade
            .commit_legacy_scheduled_agent_cutover(generation)
            .stage,
        LegacyScheduledAgentCutoverStage::Authoritative
    );
    let projected = fixture.facade.snapshot(ProjectionRequest {
        scope: ProjectionScope::ScheduledAgent { task_id: None },
        offset: 0,
        max_items: 20,
    });
    let Projection::ScheduledAgent { value } = projected.projection else {
        panic!("expected scheduled-agent projection")
    };
    assert_eq!(value.tasks.len(), 1);
    assert_eq!(value.workflows.len(), 1);
    assert_eq!(value.workflows[0].stage, ScheduledAgentStage::Succeeded);
}
