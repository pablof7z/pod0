use pod0_application::{
    ChapterContractProjection, ChapterContractRequest, ChapterModelDesiredStateInput,
    ChapterModelDesiredStatePlan, ChapterModelPlan, ChapterModelPlanInput, ChapterProjectionScope,
    TranscriptCapabilityObservation, TranscriptCapabilityRequest, TranscriptCapabilityValidation,
    TranscriptCommitRequest, TranscriptContractProjection, TranscriptProjectionScope,
    TranscriptWorkflowPlan, TranscriptWorkflowPlanInput,
};
use pod0_domain::{EpisodeId, SpeakerId};

/// Produces bounded, state-shaped evidence for the typed transcript contract.
#[uniffi::export]
pub fn project_transcript_contract(
    request: TranscriptCommitRequest,
    scope: TranscriptProjectionScope,
    offset: u32,
    max_items: u16,
) -> TranscriptContractProjection {
    pod0_application::project_transcript_contract(request, scope, offset, max_items)
}

/// Produces bounded, state-shaped evidence for the typed chapter contract.
#[uniffi::export]
pub fn project_chapter_contract(
    request: ChapterContractRequest,
    scope: ChapterProjectionScope,
    offset: u32,
    max_items: u16,
) -> ChapterContractProjection {
    pod0_application::project_chapter_contract(request, scope, offset, max_items)
}

/// Classifies whether the temporary native workflow owes model work.
#[uniffi::export]
pub fn plan_chapter_model_desired_state(
    input: ChapterModelDesiredStateInput,
) -> ChapterModelDesiredStatePlan {
    pod0_application::plan_chapter_model_desired_state(input)
}

/// Pure cross-language planner used by binding fixtures.
#[uniffi::export]
pub fn plan_chapter_model_request(input: ChapterModelPlanInput) -> ChapterModelPlan {
    pod0_application::plan_chapter_model_request(input)
}

/// Computes deterministic transcript generation and evidence-index intent.
#[uniffi::export]
pub fn plan_transcript_workflow(input: TranscriptWorkflowPlanInput) -> TranscriptWorkflowPlan {
    pod0_application::plan_transcript_workflow(input)
}

/// Validates a bounded native capability request before durable issuance.
#[uniffi::export]
pub fn validate_transcript_capability_request(
    request: TranscriptCapabilityRequest,
) -> TranscriptCapabilityValidation {
    pod0_application::validate_transcript_capability_request(request)
}

/// Validates raw native evidence before a durable state transition.
#[uniffi::export]
pub fn validate_transcript_capability_observation(
    observation: TranscriptCapabilityObservation,
) -> TranscriptCapabilityValidation {
    pod0_application::validate_transcript_capability_observation(observation)
}

/// Produces a replay-stable speaker identity without trusting native UUIDs.
#[uniffi::export]
pub fn transcript_speaker_id(
    episode_id: EpisodeId,
    source_revision: String,
    label: String,
) -> Option<SpeakerId> {
    pod0_application::transcript_speaker_id(episode_id, &source_revision, &label)
}
