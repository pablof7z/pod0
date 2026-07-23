use pod0_application::{Clock, KernelApplication, KernelProbeCommand, KernelProbeProjection};

/// An internal deterministic probe retained for injected-time characterization.
pub struct KernelProbeFacade<C> {
    application: KernelApplication<C>,
}

impl<C: Clock> KernelProbeFacade<C> {
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
