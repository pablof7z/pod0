use pod0_domain::{ContentDigest, GeneratedArtifactId, ScheduledAttemptId};
use sha2::{Digest as _, Sha256};

use crate::{
    MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES, ScheduledAgentExecutionObservation,
    ScheduledAgentExecutionRequest, StableHash,
};

#[must_use]
pub fn scheduled_generated_artifact_id(attempt_id: ScheduledAttemptId) -> GeneratedArtifactId {
    let mut hash = StableHash::new(b"pod0-scheduled-generated-artifact-v1");
    hash.bytes(&attempt_id.into_bytes());
    GeneratedArtifactId::from_bytes(hash.first_16())
}

/// Qualifies bounded provider text into canonical completion evidence. Native
/// transports return raw text only; Rust owns artifact identity and digest.
#[must_use]
pub fn qualify_scheduled_agent_completion(
    execution: &ScheduledAgentExecutionRequest,
    raw_output: &str,
) -> Option<ScheduledAgentExecutionObservation> {
    let output_size = u64::try_from(raw_output.len()).ok()?;
    if raw_output.trim().is_empty()
        || execution.maximum_output_bytes == 0
        || execution.maximum_output_bytes > MAX_SCHEDULED_AGENT_OUTPUT_EXCERPT_BYTES as u64
        || output_size > execution.maximum_output_bytes
    {
        return None;
    }
    let output_digest = ContentDigest::from_bytes(Sha256::digest(raw_output.as_bytes()).into());
    Some(ScheduledAgentExecutionObservation::Completed {
        occurrence_id: execution.occurrence_id,
        attempt_id: execution.attempt_id,
        artifact_id: scheduled_generated_artifact_id(execution.attempt_id),
        output_digest,
        output_excerpt: raw_output.to_owned(),
    })
}
