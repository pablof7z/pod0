#![forbid(unsafe_code)]

use pod0_domain::{CommandId, UnixTimestampMilliseconds};

pub const CORE_SCHEMA_VERSION: u32 = 1;

/// The kernel owns time. Hosts provide an observation through this capability;
/// reducers never sample a native or process-global clock directly.
pub trait Clock: Send + Sync {
    fn now(&self) -> UnixTimestampMilliseconds;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelProbeCommand {
    pub command_id: CommandId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelProbeProjection {
    pub command_id: CommandId,
    pub observed_at: UnixTimestampMilliseconds,
    pub core_schema_version: u32,
}

/// Minimal deterministic application boundary. It persists nothing and emits
/// no host request; the first listening slice will replace the probe with real
/// commands without changing the crate direction or time contract.
pub struct KernelApplication<C> {
    clock: C,
}

impl<C: Clock> KernelApplication<C> {
    #[must_use]
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }

    #[must_use]
    pub fn dispatch_probe(&self, command: KernelProbeCommand) -> KernelProbeProjection {
        KernelProbeProjection {
            command_id: command.command_id,
            observed_at: self.clock.now(),
            core_schema_version: CORE_SCHEMA_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct FixedClock(UnixTimestampMilliseconds);

    impl Clock for FixedClock {
        fn now(&self) -> UnixTimestampMilliseconds {
            self.0
        }
    }

    #[test]
    fn identical_command_and_time_produce_identical_projection() {
        let time = UnixTimestampMilliseconds::new(1_700_000_000_123);
        let command = KernelProbeCommand {
            command_id: CommandId::from_bytes([9; 16]),
        };

        let first = KernelApplication::new(FixedClock(time)).dispatch_probe(command);
        let second = KernelApplication::new(FixedClock(time)).dispatch_probe(command);

        assert_eq!(first, second);
        assert_eq!(first.observed_at, time);
        assert_eq!(first.core_schema_version, CORE_SCHEMA_VERSION);
    }
}
