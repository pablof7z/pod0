#![forbid(unsafe_code)]

uniffi::setup_scaffolding!();

macro_rules! opaque_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, uniffi::Record)]
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
opaque_id!(CommandId);
opaque_id!(CancellationId);
opaque_id!(SubscriptionId);
opaque_id!(HostRequestId);
opaque_id!(DomainEventId);
opaque_id!(PodcastId);
opaque_id!(EpisodeId);
opaque_id!(QueueEntryId);

mod listening;
mod listening_error;
mod listening_policy;
mod playback_policy;

pub use listening::*;
pub use listening_error::*;
pub use listening_policy::*;
pub use playback_policy::*;

#[cfg(test)]
mod listening_completion_tests;
#[cfg(test)]
mod listening_tests;
#[cfg(test)]
mod playback_policy_tests;

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
