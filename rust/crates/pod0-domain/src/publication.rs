use crate::{
    ContentDigest, EpisodeId, GeneratedArtifactId, PodcastId, PublicationId, PublicationRouteId,
    StateRevision, UnixTimestampMilliseconds,
};

/// Product-level publication identity. It is stable across process restarts
/// and distinct from NMP's store-issued receipt identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum PublicationArtifactKind {
    GeneratedPodcastEpisode,
    Unsupported { wire_code: u32 },
}

/// Public upload evidence is durable product state, not an NMP event cache.
/// The shared core validates it against the committed generated artifact
/// before composing an addressable podcast event.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct PublicationMediaEvidence {
    pub public_url: String,
    pub media_type: String,
    pub byte_count: u64,
    pub content_digest: ContentDigest,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct PublicationIntent {
    pub artifact_id: GeneratedArtifactId,
    pub kind: PublicationArtifactKind,
    /// Lowercase 32-byte hexadecimal x-only public key.
    pub expected_author_hex: String,
    pub semantic_revision: u32,
    pub media: PublicationMediaEvidence,
}

/// Exact durable status facts remain distinct. No variant means globally
/// synced or delivered; an acknowledgement is evidence for one opaque route.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum PublicationFactKind {
    Accepted,
    Cancelled,
    AwaitingCapability,
    Signed,
    Routed,
    AwaitingRelay,
    AwaitingAuth,
    RetryEligible,
    HandoffAmbiguous,
    Sent,
    Acknowledged,
    Rejected,
    GaveUp,
    PersistenceBlocked,
    RoutePersistenceBlocked,
    OutcomeUnknown,
    ReplaceableConflict,
    Failed,
    ReattachmentNotFound,
    ReattachmentUnreadable,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct PublicationFact {
    pub sequence: u64,
    pub kind: PublicationFactKind,
    /// A stable digest of a relay URL. The URL itself never crosses the
    /// Pod0 application facade.
    pub route_id: Option<PublicationRouteId>,
    pub attempt: Option<u64>,
    pub event_id_hex: Option<String>,
    pub observed_at: Option<UnixTimestampMilliseconds>,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum PublicationStage {
    Prepared,
    AwaitingCapability,
    Signing,
    Routed,
    Delivering,
    EvidenceMixed,
    Acknowledged,
    Rejected,
    Blocked,
    OutcomeUnknown,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct PublicationRecord {
    pub publication_id: PublicationId,
    pub artifact_id: GeneratedArtifactId,
    pub artifact_kind: PublicationArtifactKind,
    pub episode_id: EpisodeId,
    pub podcast_id: PodcastId,
    pub semantic_revision: u32,
    pub revision: StateRevision,
    pub expected_author_hex: String,
    pub correlation_token: String,
    pub media: PublicationMediaEvidence,
    pub receipt_id: Option<u64>,
    pub event_id_hex: Option<String>,
    pub stage: PublicationStage,
    pub prepared_at: UnixTimestampMilliseconds,
    pub updated_at: UnixTimestampMilliseconds,
    pub facts: Vec<PublicationFact>,
}
