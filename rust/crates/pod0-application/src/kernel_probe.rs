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
