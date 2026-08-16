use pod0_domain::ContentDigest;
use sha2::{Digest as _, Sha256};

use crate::{DownloadLeasedObservationAction, DownloadObservationCommitInput};

pub(super) fn fingerprint(input: &DownloadObservationCommitInput) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0/download-observation-ingress/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.observation.request_id.into_bytes());
    hash.update(input.observation.sequence_number.to_be_bytes());
    match &input.action {
        DownloadLeasedObservationAction::Accepted {
            external_task_key,
            resume_key,
        } => {
            hash.update([1]);
            field(&mut hash, external_task_key.as_bytes());
            optional_field(&mut hash, resume_key.as_deref().map(str::as_bytes));
        }
        DownloadLeasedObservationAction::Cancellation => hash.update([2]),
        DownloadLeasedObservationAction::Removal { artifact_key } => {
            hash.update([3]);
            field(&mut hash, artifact_key.as_bytes());
        }
        DownloadLeasedObservationAction::Staged {
            staged_file_path,
            claimed_byte_count,
        } => {
            hash.update([4]);
            field(&mut hash, staged_file_path.as_bytes());
            hash.update(claimed_byte_count.to_be_bytes());
        }
        DownloadLeasedObservationAction::Failure(failure) => {
            hash.update([5]);
            field(&mut hash, failure.failure_code.as_bytes());
            optional_field(
                &mut hash,
                failure.failure_detail.as_deref().map(str::as_bytes),
            );
            hash.update([u8::from(failure.retryable)]);
            optional_i64(&mut hash, failure.retry_at_ms);
            optional_i64(&mut hash, failure.retry_deadline_at_ms);
            hash.update(failure.issued_revision.value.to_be_bytes());
            hash.update(failure.observed_at_ms.to_be_bytes());
        }
    }
    ContentDigest::from_bytes(hash.finalize().into())
}

fn field(hash: &mut Sha256, value: &[u8]) {
    hash.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hash.update(value);
}

fn optional_field(hash: &mut Sha256, value: Option<&[u8]>) {
    hash.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        field(hash, value);
    }
}

fn optional_i64(hash: &mut Sha256, value: Option<i64>) {
    hash.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        hash.update(value.to_be_bytes());
    }
}
