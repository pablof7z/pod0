use pod0_domain::{
    CommandId, ContentDigest, EpisodeId, FeedDiscoveryOccurrenceId, PodcastId,
    UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

pub const MAX_LEGACY_FEED_DISCOVERY_CANDIDATES: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedDiscoveryCutoverState {
    NotStarted,
    Staged { source_generation: u64 },
    Authoritative { source_generation: u64 },
}

impl FeedDiscoveryCutoverState {
    pub const fn source_generation(self) -> Option<u64> {
        match self {
            Self::NotStarted => None,
            Self::Staged { source_generation } | Self::Authoritative { source_generation } => {
                Some(source_generation)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyFeedDiscoveryEffectKind {
    Download,
    Notification,
}

impl LegacyFeedDiscoveryEffectKind {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::Notification => "notification",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyFeedDiscoveryDisposition {
    Pending,
    Succeeded,
    Obsolete,
    Failed,
    Ambiguous,
}

impl LegacyFeedDiscoveryDisposition {
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Succeeded => "succeeded",
            Self::Obsolete => "obsolete",
            Self::Failed => "failed",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyFeedDiscoveryCandidate {
    pub occurrence_id: FeedDiscoveryOccurrenceId,
    pub command_id: CommandId,
    pub podcast_id: PodcastId,
    pub episode_id: EpisodeId,
    pub kind: LegacyFeedDiscoveryEffectKind,
    pub disposition: LegacyFeedDiscoveryDisposition,
    pub attempt: u8,
    pub not_before: Option<UnixTimestampMilliseconds>,
    pub observed_at: UnixTimestampMilliseconds,
    pub expires_at: UnixTimestampMilliseconds,
    pub published_at: UnixTimestampMilliseconds,
    pub input_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyFeedDiscoveryCutoverInput {
    pub backup_digest: ContentDigest,
    pub backup_byte_count: u64,
    pub notification_command_id: CommandId,
    pub notifications_enabled: bool,
    pub inspected_job_count: u32,
    pub blocked_count: u32,
    pub ambiguous_count: u32,
    pub candidates: Vec<LegacyFeedDiscoveryCandidate>,
    pub observed_at: UnixTimestampMilliseconds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyFeedDiscoveryCutoverReport {
    pub state: FeedDiscoveryCutoverState,
    pub source_fingerprint: Option<ContentDigest>,
    pub backup_digest: Option<ContentDigest>,
    pub backup_byte_count: Option<u64>,
    pub notifications_enabled: Option<bool>,
    pub inspected_job_count: u32,
    pub candidate_count: u32,
    pub blocked_count: u32,
    pub ambiguous_count: u32,
}

pub fn feed_discovery_cutover_source_fingerprint(
    input: &LegacyFeedDiscoveryCutoverInput,
) -> ContentDigest {
    let mut hash = StableCutoverHash::new();
    hash.bytes(&input.backup_digest.into_bytes());
    hash.u64(input.backup_byte_count);
    hash.bytes(&input.notification_command_id.into_bytes());
    hash.bytes(&[u8::from(input.notifications_enabled)]);
    hash.u64(u64::from(input.inspected_job_count));
    hash.u64(u64::from(input.blocked_count));
    hash.u64(u64::from(input.ambiguous_count));
    let mut candidates: Vec<_> = input.candidates.iter().collect();
    candidates.sort_by_key(|candidate| {
        (
            candidate.occurrence_id.into_bytes(),
            candidate.episode_id.into_bytes(),
            candidate.kind.wire(),
        )
    });
    hash.u64(candidates.len() as u64);
    for candidate in candidates {
        hash_candidate(&mut hash, candidate);
    }
    ContentDigest::from_bytes(hash.finish())
}

pub fn feed_discovery_cutover_source_generation(fingerprint: ContentDigest) -> u64 {
    let bytes = fingerprint.into_bytes();
    u64::from_be_bytes(bytes[..8].try_into().expect("digest prefix")) & i64::MAX as u64 | 1
}

fn hash_candidate(hash: &mut StableCutoverHash, value: &LegacyFeedDiscoveryCandidate) {
    hash.bytes(&value.occurrence_id.into_bytes());
    hash.bytes(&value.command_id.into_bytes());
    hash.bytes(&value.podcast_id.into_bytes());
    hash.bytes(&value.episode_id.into_bytes());
    hash.text(value.kind.wire());
    hash.text(value.disposition.wire());
    hash.u64(u64::from(value.attempt));
    hash.optional_i64(value.not_before.map(|time| time.value()));
    hash.i64(value.observed_at.value());
    hash.i64(value.expires_at.value());
    hash.i64(value.published_at.value());
    hash.text(&value.input_version);
}

struct StableCutoverHash(Sha256);

impl StableCutoverHash {
    fn new() -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(b"pod0-legacy-feed-discovery-cutover-v1");
        value
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.bytes(&value.to_be_bytes());
    }

    fn optional_i64(&mut self, value: Option<i64>) {
        match value {
            Some(value) => {
                self.bytes(&[1]);
                self.i64(value);
            }
            None => self.bytes(&[0]),
        }
    }

    fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}
