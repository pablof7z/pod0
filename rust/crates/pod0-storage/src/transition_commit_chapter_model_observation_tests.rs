use pod0_application::EffectOutcome;
use pod0_domain::{ChapterModelSubmissionFenceId, EpisodeId, HostRequestId};

use super::chapter_model_observation::effect_outcome;
use crate::{ModelChapterObservationAction, ModelChapterProviderAcceptedInput};

#[test]
fn provider_acceptance_is_progress_not_success_or_unknown() {
    let action =
        ModelChapterObservationAction::ProviderAccepted(ModelChapterProviderAcceptedInput {
            episode_id: EpisodeId::from_parts(1, 1),
            request_id: HostRequestId::from_parts(2, 2),
            generation: 3,
            submission_fence_id: ChapterModelSubmissionFenceId::from_parts(4, 4),
            provider_operation_id: "operation-1".to_owned(),
            provider_status: Some("queued".to_owned()),
            observed_at_ms: 5,
        });

    assert_eq!(effect_outcome(&action), EffectOutcome::Progressed);
}
