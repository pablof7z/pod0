use pod0_application::ScheduledAgentExecutionObservation;
use sha2::{Digest as _, Sha256};

pub(crate) fn observation_fingerprint(value: &ScheduledAgentExecutionObservation) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"pod0-scheduled-observation-v1");
    match value {
        ScheduledAgentExecutionObservation::Accepted {
            occurrence_id,
            attempt_id,
            provider_operation_id,
        } => {
            hash.update([1]);
            hash.update(occurrence_id.into_bytes());
            hash.update(attempt_id.into_bytes());
            hash_optional_text(&mut hash, provider_operation_id.as_deref());
        }
        ScheduledAgentExecutionObservation::Completed {
            occurrence_id,
            attempt_id,
            artifact_id,
            output_digest,
            output_excerpt,
        } => {
            hash.update([2]);
            hash.update(occurrence_id.into_bytes());
            hash.update(attempt_id.into_bytes());
            hash.update(artifact_id.into_bytes());
            hash.update(output_digest.into_bytes());
            hash_text(&mut hash, output_excerpt);
        }
        ScheduledAgentExecutionObservation::Failed {
            occurrence_id,
            attempt_id,
            code,
            safe_detail,
            retry_after_milliseconds,
        } => {
            hash.update([3]);
            hash.update(occurrence_id.into_bytes());
            hash.update(attempt_id.into_bytes());
            let (name, wire) = crate::scheduled_agent_store_codec::failure_wire(*code);
            hash_text(&mut hash, name);
            hash.update(wire.unwrap_or(-1).to_be_bytes());
            hash_optional_text(&mut hash, safe_detail.as_deref());
            hash.update(retry_after_milliseconds.unwrap_or(u64::MAX).to_be_bytes());
        }
        ScheduledAgentExecutionObservation::Cancelled {
            occurrence_id,
            attempt_id,
        } => {
            hash.update([4]);
            hash.update(occurrence_id.into_bytes());
            hash.update(attempt_id.into_bytes());
        }
        ScheduledAgentExecutionObservation::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
    hash.finalize().into()
}

fn hash_optional_text(hash: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash.update([1]);
            hash_text(hash, value);
        }
        None => hash.update([0]),
    }
}

fn hash_text(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
}
