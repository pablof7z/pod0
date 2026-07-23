use crate::{SignerAccountId, StateRevision, UnixTimestampMilliseconds};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum SignerCredentialKind {
    LocalKeychain,
    RemoteNip46,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum SignerStage {
    Provisioning,
    Restoring,
    Ready,
    Unavailable,
    SigningOut,
    Failed,
}

/// Durable product identity metadata. Secret material is never part of this
/// record and remains in the platform secure-storage capability.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct SignerAccountRecord {
    pub account_id: Option<SignerAccountId>,
    pub credential_kind: SignerCredentialKind,
    pub expected_author_hex: Option<String>,
    pub revision: StateRevision,
    pub stage: SignerStage,
    pub updated_at: UnixTimestampMilliseconds,
    pub safe_detail: Option<String>,
}
