#![forbid(unsafe_code)]

use std::sync::Arc;

pub use pod0_application::{
    ApplicationCommand, CommandEnvelope, CoreFailure, CoreFailureCode, DomainEvent,
    DomainEventEnvelope, EpisodeSummary, FACADE_CONTRACT_VERSION, HostFailureCode, HostObservation,
    HostObservationEnvelope, HostRequest, HostRequestEnvelope, KernelProbeCommand,
    KernelProbeProjection, LibraryProjection, MAX_FEED_RESPONSE_BYTES, MAX_HOST_REQUEST_BATCH,
    MAX_OPERATION_ITEMS, MAX_PROJECTION_ITEMS, OperationProjection, OperationStage, PlaybackItem,
    PlaybackPolicyState, PlaybackProjection, PlaybackStopReason, PodcastSummary, Projection,
    ProjectionEnvelope, ProjectionRequest, ProjectionScope, Retryability, UnsupportedProjection,
    UserAction, bounded_host_request_count,
};
use pod0_application::{Clock, KernelApplication};
pub use pod0_domain::{
    CancellationId, CommandId, DomainEventId, EpisodeId, HostRequestId, PodcastId, StateRevision,
    SubscriptionId, UnixTimestampMilliseconds,
};

uniffi::setup_scaffolding!();

mod runtime;
mod runtime_state;
#[cfg(test)]
mod runtime_tests;
pub use runtime::Pod0Facade;

#[derive(Debug, uniffi::Error)]
pub enum ProjectionDeliveryError {
    CallbackFailed { safe_message: String },
    UnexpectedCallback,
}

impl std::fmt::Display for ProjectionDeliveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CallbackFailed { safe_message } => formatter.write_str(safe_message),
            Self::UnexpectedCallback => formatter.write_str("projection callback failed"),
        }
    }
}

impl std::error::Error for ProjectionDeliveryError {}

impl From<uniffi::UnexpectedUniFFICallbackError> for ProjectionDeliveryError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::UnexpectedCallback
    }
}

/// Event-driven projection delivery. The generated Swift and Kotlin callback
/// interfaces derive from this single app-owned surface.
#[uniffi::export(with_foreign)]
pub trait ProjectionSubscriber: Send + Sync {
    fn receive(&self, projection: ProjectionEnvelope) -> Result<(), ProjectionDeliveryError>;
}

/// Shape of the one native/core API. Dispatch and host observation methods do
/// not return per-operation success; durable outcomes appear in projections.
pub trait Pod0ApplicationApi: Send + Sync {
    fn dispatch(&self, command: CommandEnvelope);
    fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope;
    fn subscribe(
        &self,
        request: ProjectionRequest,
        subscriber: Arc<dyn ProjectionSubscriber>,
    ) -> SubscriptionId;
    fn unsubscribe(&self, subscription_id: SubscriptionId);
    fn next_host_requests(&self, maximum_count: u16) -> Vec<HostRequestEnvelope>;
    fn record_host_observation(&self, observation: HostObservationEnvelope);
}

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

#[cfg(test)]
mod tests {
    use super::*;

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

        let projection = KernelProbeFacade::new(FixedClock).dispatch_probe(command);

        assert_eq!(projection.command_id, command.command_id);
        assert_eq!(projection.observed_at.value(), 42);
    }

    #[test]
    fn listening_actions_are_typed_without_dynamic_dispatch() {
        let command = CommandEnvelope {
            command_id: CommandId::from_parts(0, 1),
            cancellation_id: CancellationId::from_parts(0, 2),
            expected_revision: Some(StateRevision::new(3)),
            command: ApplicationCommand::RequestPlayback {
                episode_id: EpisodeId::from_parts(0, 4),
            },
        };

        assert!(matches!(
            command.command,
            ApplicationCommand::RequestPlayback { episode_id }
                if episode_id == EpisodeId::from_parts(0, 4)
        ));
        assert_eq!(bounded_host_request_count(0), 1);
        assert_eq!(
            bounded_host_request_count(u16::MAX),
            usize::from(MAX_HOST_REQUEST_BATCH)
        );
    }
}
