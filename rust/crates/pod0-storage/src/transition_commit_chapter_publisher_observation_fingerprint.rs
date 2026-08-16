use pod0_application::{HostFailureCode, HostObservation};
use pod0_domain::ContentDigest;
use sha2::{Digest as _, Sha256};

use crate::{PublisherChapterObservationCommitInput, StorageError};

pub(super) fn fingerprint(
    input: &PublisherChapterObservationCommitInput,
) -> Result<ContentDigest, StorageError> {
    let mut hash = Sha256::new();
    hash.update(b"pod0/chapter/publisher-observation/v1");
    hash.update(input.lease.intent_id.into_bytes());
    hash.update(input.lease.attempt_id.into_bytes());
    hash.update(input.observation.request_id.into_bytes());
    hash.update(input.observation.sequence_number.to_be_bytes());
    match &input.observation.observation {
        HostObservation::PublisherChaptersFetched {
            episode_id,
            bytes,
            content_type,
            response_url,
            entity_tag,
            last_modified,
            http_status,
        } => {
            hash.update([1]);
            hash.update(episode_id.into_bytes());
            hash.update(Sha256::digest(bytes));
            hash_text(&mut hash, content_type);
            hash_text(&mut hash, response_url);
            hash_optional_text(&mut hash, entity_tag.as_deref());
            hash_optional_text(&mut hash, last_modified.as_deref());
            hash.update(http_status.to_be_bytes());
        }
        HostObservation::Failed { code, safe_detail } => {
            hash.update([2]);
            hash_host_failure(&mut hash, code);
            hash_optional_text(&mut hash, safe_detail.as_deref());
        }
        HostObservation::Cancelled => hash.update([3]),
        HostObservation::Unsupported { wire_code } => {
            hash.update([4]);
            hash.update(wire_code.to_be_bytes());
        }
        _ => return Err(StorageError::InvalidActivity),
    }
    Ok(ContentDigest::from_bytes(hash.finalize().into()))
}

fn hash_text(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
}

fn hash_optional_text(hash: &mut Sha256, value: Option<&str>) {
    hash.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        hash_text(hash, value);
    }
}

fn hash_host_failure(hash: &mut Sha256, code: &HostFailureCode) {
    match code {
        HostFailureCode::Offline => hash.update([1]),
        HostFailureCode::TimedOut => hash.update([2]),
        HostFailureCode::PermissionDenied => hash.update([3]),
        HostFailureCode::InvalidResponse => hash.update([4]),
        HostFailureCode::ResponseTooLarge => hash.update([5]),
        HostFailureCode::MediaUnavailable => hash.update([6]),
        HostFailureCode::ProviderUnavailable => hash.update([7]),
        HostFailureCode::Unauthorized => hash.update([8]),
        HostFailureCode::IndexUnavailable => hash.update([9]),
        HostFailureCode::PlatformFailure => hash.update([10]),
        HostFailureCode::Unsupported { wire_code } => {
            hash.update([255]);
            hash.update(wire_code.to_be_bytes());
        }
    }
}
