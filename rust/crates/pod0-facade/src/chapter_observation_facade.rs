use crate::{
    AgentComposedChapterObservation, ChapterObservationProjection, ModelChapterObservation,
    PublisherChapterObservation,
};

/// Qualifies bounded publisher bytes without network or persistence effects.
#[uniffi::export]
pub fn qualify_publisher_chapter_observation(
    observation: PublisherChapterObservation,
) -> ChapterObservationProjection {
    pod0_application::qualify_publisher_chapter_observation(observation)
}

/// Qualifies bounded generated or enriched model output as state.
#[uniffi::export]
pub fn qualify_model_chapter_observation(
    observation: ModelChapterObservation,
) -> ChapterObservationProjection {
    pod0_application::qualify_model_chapter_observation(observation)
}

/// Qualifies ordered agent-composed chapter evidence as state.
#[uniffi::export]
pub fn qualify_agent_composed_chapter_observation(
    observation: AgentComposedChapterObservation,
) -> ChapterObservationProjection {
    pod0_application::qualify_agent_composed_chapter_observation(observation)
}
