#![forbid(unsafe_code)]

/// Opaque command identity supplied by the caller. The core never generates
/// identifiers while replaying deterministic application behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandId([u8; 16]);

impl CommandId {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Kernel-owned time representation with an explicit unit and no platform
/// date type at the shared boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnixTimestampMilliseconds(i64);

impl UnixTimestampMilliseconds {
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
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
        assert_eq!(timestamp.value(), 1_700_000_000_123);
    }
}
