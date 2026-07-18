#![forbid(unsafe_code)]

use pod0_application::{Clock, KernelApplication};
pub use pod0_application::{KernelProbeCommand, KernelProbeProjection};

/// The sole app-owned native/core boundary. This bootstrap wrapper proves the
/// dependency direction; issue #74 adds the real bounded listening contract.
pub struct Pod0Facade<C> {
    application: KernelApplication<C>,
}

impl<C: Clock> Pod0Facade<C> {
    #[must_use]
    pub const fn new(clock: C) -> Self {
        Self {
            application: KernelApplication::new(clock),
        }
    }

    #[must_use]
    pub fn dispatch_probe(&self, command: KernelProbeCommand) -> KernelProbeProjection {
        self.application.dispatch_probe(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod0_domain::{CommandId, UnixTimestampMilliseconds};

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> UnixTimestampMilliseconds {
            UnixTimestampMilliseconds::new(42)
        }
    }

    #[test]
    fn facade_preserves_the_typed_application_projection() {
        let command = KernelProbeCommand {
            command_id: CommandId::from_bytes([4; 16]),
        };

        let projection = Pod0Facade::new(FixedClock).dispatch_probe(command);

        assert_eq!(projection.command_id, command.command_id);
        assert_eq!(projection.observed_at.value(), 42);
    }
}
