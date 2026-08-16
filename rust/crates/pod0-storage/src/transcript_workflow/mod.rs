mod authority;
mod completion;
mod completion_stage;
mod cutover;
mod cutover_adoption;
mod cutover_adoption_state;
mod cutover_model;
mod cutover_rows;
mod cutover_stage;
mod cutover_validation;
mod failure;
mod model;
mod observation_model;
mod persist;
mod read;
mod recovery;
mod store;
mod submission;
mod submission_observation;
mod support;

pub(crate) use authority::require_authoritative;
pub(crate) use completion::{
    apply_transcript_evidence_completion, apply_transcript_workflow_commit,
};
pub(crate) use completion_stage::apply_transcript_completion;
pub use cutover::*;
pub use cutover_stage::transcript_workflow_source_fingerprint;
pub(crate) use failure::apply_transcript_failure;
pub(crate) use failure::apply_transcript_workflow_cancellation;
pub use model::*;
pub use observation_model::*;
pub(crate) use read::read_workflow;
pub(crate) use recovery::apply_ambiguous_recovery;
pub(crate) use store::{apply_transcript_workflow_ensure, replays, validate_ensure};
pub(crate) use submission::apply_transcript_provider_accepted;
pub(crate) use submission::{authorize_submission, exact_attempt, validate_claim};
pub(crate) use submission_observation::apply_transcript_provider_pending;

#[cfg(test)]
mod authority_crash_tests;
#[cfg(test)]
mod crash_tests;
#[cfg(test)]
mod cutover_tests;
#[cfg(test)]
mod recovery_fence_tests;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod stage_restart_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
