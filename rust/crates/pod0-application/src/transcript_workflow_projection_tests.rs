use pod0_domain::{EpisodeId, StateRevision, TranscriptWorkflowId, UnixTimestampMilliseconds};

use super::*;

#[test]
fn workflow_projection_is_bounded() {
    let workflow_id = TranscriptWorkflowId::from_bytes([7; 16]);
    let mut page = TranscriptWorkflowsProjection {
        workflows: (0..205)
            .map(|index| TranscriptWorkflowProjection {
                episode_id: EpisodeId::from_parts(0, index),
                workflow_id,
                source_revision: "audio-v1".to_owned(),
                origin: TranscriptWorkflowOrigin::Automatic,
                provider: TranscriptProvider::AssemblyAi,
                model: "model".to_owned(),
                stage: TranscriptWorkflowStage::Requested,
                workflow_revision: StateRevision::new(index),
                attempt: 0,
                attempt_id: None,
                submission_fence_id: None,
                request_id: None,
                external_operation_present: false,
                not_before: None,
                failure: None,
                updated_at: UnixTimestampMilliseconds::new(index as i64),
                allowed_actions: transcript_allowed_actions(TranscriptWorkflowStage::Requested),
                retry_action: None,
                cancel_action: None,
            })
            .collect(),
        has_more: false,
        failure: None,
    };
    page.enforce_bounds(2, u16::MAX as usize);
    assert_eq!(page.workflows.len(), 200);
    assert!(page.has_more);
}
