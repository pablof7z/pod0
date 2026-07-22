#![forbid(unsafe_code)]

uniffi::setup_scaffolding!();

macro_rules! opaque_id {
    ($name:ident $(, $extra:path)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash $(, $extra)*)]
        pub struct $name {
            pub high: u64,
            pub low: u64,
        }

        impl $name {
            #[must_use]
            pub const fn from_parts(high: u64, low: u64) -> Self {
                Self { high, low }
            }

            #[must_use]
            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self {
                    high: u64::from_be_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
                        bytes[7],
                    ]),
                    low: u64::from_be_bytes([
                        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
                        bytes[15],
                    ]),
                }
            }

            #[must_use]
            pub const fn into_bytes(self) -> [u8; 16] {
                let high = self.high.to_be_bytes();
                let low = self.low.to_be_bytes();
                [
                    high[0], high[1], high[2], high[3], high[4], high[5], high[6], high[7], low[0],
                    low[1], low[2], low[3], low[4], low[5], low[6], low[7],
                ]
            }

            #[must_use]
            pub const fn high(self) -> u64 {
                self.high
            }

            #[must_use]
            pub const fn low(self) -> u64 {
                self.low
            }
        }
    };
}

// Opaque identities cross the native/core boundary as two unsigned 64-bit
// words. Their meaning remains domain-specific rather than stringly typed.
opaque_id!(CommandId, uniffi::Record);
opaque_id!(CancellationId, uniffi::Record);
opaque_id!(SubscriptionId, uniffi::Record);
opaque_id!(HostRequestId, uniffi::Record);
opaque_id!(DomainEventId, uniffi::Record);
opaque_id!(PodcastId, uniffi::Record);
opaque_id!(EpisodeId, uniffi::Record);
opaque_id!(QueueEntryId, uniffi::Record);
opaque_id!(RecallQueryId, uniffi::Record);
opaque_id!(SpeakerId, uniffi::Record);
opaque_id!(TranscriptArtifactId, uniffi::Record);
opaque_id!(TranscriptVersionId, uniffi::Record);
opaque_id!(TranscriptSegmentId, uniffi::Record);
opaque_id!(ChapterArtifactId, uniffi::Record);
opaque_id!(ChapterId, uniffi::Record);
opaque_id!(AdSpanId, uniffi::Record);
opaque_id!(ChapterPlaybackSessionId, uniffi::Record);
opaque_id!(ChapterModelSubmissionFenceId, uniffi::Record);
opaque_id!(DownloadIntentId, uniffi::Record);
opaque_id!(DownloadAttemptId, uniffi::Record);
opaque_id!(EvidenceSpanId, uniffi::Record);
opaque_id!(EvidenceGenerationId, uniffi::Record);
opaque_id!(NoteId, uniffi::Record);
opaque_id!(ClipId, uniffi::Record);

mod chapter_artifact;
mod chapter_artifact_hash;
mod chapter_artifact_validation;
mod chapter_playback_policy;
mod clips;
mod download_identity;
mod knowledge;
mod knowledge_artifact;
mod knowledge_artifact_hash;
mod knowledge_identity;
mod listening;
mod listening_error;
mod listening_policy;
mod notes;
mod playback_policy;
mod recall_configuration;
mod transcript_artifact;
mod transcript_artifact_hash;
mod transcript_artifact_validation;
mod transcript_command;

pub use chapter_artifact::*;
pub use chapter_playback_policy::*;
pub use clips::*;
pub use download_identity::*;
pub use knowledge::*;
pub use knowledge_identity::*;
pub use listening::*;
pub use listening_error::*;
pub use listening_policy::*;
pub use notes::*;
pub use playback_policy::*;
pub use recall_configuration::*;
pub use transcript_artifact::*;
pub use transcript_command::*;

#[cfg(test)]
mod chapter_artifact_tests;
#[cfg(test)]
mod chapter_playback_policy_tests;
#[cfg(test)]
mod listening_completion_tests;
#[cfg(test)]
mod listening_tests;
#[cfg(test)]
mod playback_policy_tests;
#[cfg(test)]
mod transcript_artifact_tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Record)]
pub struct StateRevision {
    pub value: u64,
}

impl StateRevision {
    pub const INITIAL: Self = Self { value: 0 };

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self { value }
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }
}

/// Kernel-owned time representation with an explicit unit and no platform
/// date type at the shared boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, uniffi::Record)]
pub struct UnixTimestampMilliseconds {
    pub value: i64,
}

impl UnixTimestampMilliseconds {
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self { value }
    }

    #[must_use]
    pub const fn value(self) -> i64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_types_preserve_exact_caller_supplied_values() {
        let id = CommandId::from_bytes([7; 16]);
        let timestamp = UnixTimestampMilliseconds::new(1_700_000_000_123);

        assert_eq!(id.into_bytes(), [7; 16]);
        assert_eq!(CommandId::from_parts(id.high(), id.low()), id);
        assert_eq!(timestamp.value(), 1_700_000_000_123);
        assert_eq!(StateRevision::INITIAL.value(), 0);
    }
}
