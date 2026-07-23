use pod0_domain::{SignerAccountId, SignerAccountRecord};

use crate::OperationProjection;

pub const MAX_SIGNER_CONTENT_BYTES: usize = 65_536;
pub const MAX_SIGNER_TAGS: usize = 32;
pub const MAX_SIGNER_TAG_VALUE_BYTES: usize = 8_192;
pub const MAX_PENDING_SIGNER_REQUESTS: usize = 4;
pub const SIGNER_HOST_DEADLINE_MILLISECONDS: i64 = 120_000;

/// Exact immutable template NMP has frozen for a native signing capability.
/// The native host returns only the signature and event id; NMP independently
/// verifies the complete event before promotion.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NostrSigningRequest {
    pub account_id: SignerAccountId,
    pub event_id_hex: String,
    pub expected_author_hex: String,
    pub created_at_seconds: u64,
    pub kind: u16,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NostrSignatureObservation {
    pub account_id: SignerAccountId,
    pub event_id_hex: String,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SignerProjection {
    pub account: Option<SignerAccountRecord>,
    pub pending_request_count: u16,
    pub operations: Vec<OperationProjection>,
}

#[must_use]
pub fn valid_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[must_use]
pub fn signing_request_is_bounded(request: &NostrSigningRequest) -> bool {
    valid_lower_hex(&request.event_id_hex, 64)
        && valid_lower_hex(&request.expected_author_hex, 64)
        && request.content.len() <= MAX_SIGNER_CONTENT_BYTES
        && request.tags.len() <= MAX_SIGNER_TAGS
        && request.tags.iter().all(|tag| {
            !tag.is_empty()
                && tag
                    .iter()
                    .all(|value| value.len() <= MAX_SIGNER_TAG_VALUE_BYTES)
        })
}

#[must_use]
pub fn signature_observation_is_valid(observation: &NostrSignatureObservation) -> bool {
    valid_lower_hex(&observation.event_id_hex, 64)
        && valid_lower_hex(&observation.signature_hex, 128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signer_payload_bounds_reject_secret_shaped_or_unbounded_values() {
        let account_id = SignerAccountId::from_bytes([1; 16]);
        let request = NostrSigningRequest {
            account_id,
            event_id_hex: "a".repeat(64),
            expected_author_hex: "b".repeat(64),
            created_at_seconds: 1,
            kind: 1,
            tags: vec![vec!["t".into(), "pod0".into()]],
            content: "bounded".into(),
        };
        assert!(signing_request_is_bounded(&request));
        assert!(!signing_request_is_bounded(&NostrSigningRequest {
            content: "x".repeat(MAX_SIGNER_CONTENT_BYTES + 1),
            ..request.clone()
        }));
        assert!(signature_observation_is_valid(&NostrSignatureObservation {
            account_id,
            event_id_hex: request.event_id_hex,
            signature_hex: "c".repeat(128),
        }));
    }
}
