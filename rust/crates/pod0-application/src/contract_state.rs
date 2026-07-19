use std::collections::BTreeMap;

use pod0_domain::{CancellationId, CommandId, HostRequestId, StateRevision, SubscriptionId};

use crate::contract_state_validation::{observation_matches_request, recall_payload_is_bounded};
use crate::{
    CommandEnvelope, HostObservation, HostObservationEnvelope, HostRequest, HostRequestEnvelope,
    ProjectionRequest,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandRegistration {
    Accepted,
    Duplicate,
    ConflictingReuse,
    StaleRevision,
}

#[derive(Default)]
pub struct CommandLedger {
    commands: BTreeMap<CommandId, CommandEnvelope>,
}

impl CommandLedger {
    pub fn register(
        &mut self,
        command: CommandEnvelope,
        current_revision: StateRevision,
    ) -> CommandRegistration {
        if let Some(existing) = self.commands.get(&command.command_id) {
            return if existing == &command {
                CommandRegistration::Duplicate
            } else {
                CommandRegistration::ConflictingReuse
            };
        }
        let is_stale = command
            .expected_revision
            .is_some_and(|expected| expected != current_revision);
        self.commands.insert(command.command_id, command);
        if is_stale {
            CommandRegistration::StaleRevision
        } else {
            CommandRegistration::Accepted
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostRequestStatus {
    Outstanding,
    Cancelled,
    Completed,
}

struct TrackedHostRequest {
    envelope: HostRequestEnvelope,
    status: HostRequestStatus,
    last_sequence_number: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationAcceptance {
    Accepted,
    UnknownRequest,
    Duplicate,
    Cancelled,
    CancellationMismatch,
    StaleRequestRevision,
    OutOfOrder,
    MismatchedPayload,
    PayloadTooLarge,
}

#[derive(Default)]
pub struct HostRequestLedger {
    requests: BTreeMap<HostRequestId, TrackedHostRequest>,
}

impl HostRequestLedger {
    #[must_use]
    pub fn command_id(&self, request_id: HostRequestId) -> Option<CommandId> {
        self.requests
            .get(&request_id)
            .map(|request| request.envelope.command_id)
    }

    #[must_use]
    pub fn is_playback_observation_stream(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::ObservePlayback { .. }
            )
        })
    }

    #[must_use]
    pub fn is_playback_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::LoadMedia { .. }
                    | HostRequest::Play { .. }
                    | HostRequest::Pause { .. }
                    | HostRequest::Seek { .. }
                    | HostRequest::SetRate { .. }
                    | HostRequest::ArmNativeTimer { .. }
                    | HostRequest::CancelNativeTimer { .. }
                    | HostRequest::ObservePlayback { .. }
                    | HostRequest::StopPlayback { .. }
            )
        })
    }

    #[must_use]
    pub fn register(&mut self, request: HostRequestEnvelope) -> bool {
        if self.requests.contains_key(&request.request_id) {
            return false;
        }
        self.requests.insert(
            request.request_id,
            TrackedHostRequest {
                envelope: request,
                status: HostRequestStatus::Outstanding,
                last_sequence_number: None,
            },
        );
        true
    }

    pub fn cancel(&mut self, cancellation_id: CancellationId) -> usize {
        let mut cancelled = 0;
        for request in self.requests.values_mut() {
            if request.envelope.cancellation_id == cancellation_id
                && request.status == HostRequestStatus::Outstanding
            {
                request.status = HostRequestStatus::Cancelled;
                cancelled += 1;
            }
        }
        cancelled
    }

    pub fn accept_observation(
        &mut self,
        observation: &HostObservationEnvelope,
    ) -> ObservationAcceptance {
        let Some(request) = self.requests.get_mut(&observation.request_id) else {
            return ObservationAcceptance::UnknownRequest;
        };
        if request.status == HostRequestStatus::Completed {
            return ObservationAcceptance::Duplicate;
        }
        if request.envelope.cancellation_id != observation.cancellation_id {
            return ObservationAcceptance::CancellationMismatch;
        }
        if request.envelope.issued_revision != observation.observed_request_revision {
            return ObservationAcceptance::StaleRequestRevision;
        }
        if request
            .last_sequence_number
            .is_some_and(|sequence| observation.sequence_number <= sequence)
        {
            return if request.last_sequence_number == Some(observation.sequence_number) {
                ObservationAcceptance::Duplicate
            } else {
                ObservationAcceptance::OutOfOrder
            };
        }
        if request.status == HostRequestStatus::Cancelled {
            return ObservationAcceptance::Cancelled;
        }
        if !observation_matches_request(&request.envelope.request, &observation.observation) {
            return ObservationAcceptance::MismatchedPayload;
        }
        if let (
            HostRequest::FetchFeed {
                maximum_response_bytes,
                ..
            },
            HostObservation::FeedBytesFetched { bytes, .. },
        ) = (&request.envelope.request, &observation.observation)
            && u64::try_from(bytes.len()).map_or(true, |size| size > *maximum_response_bytes)
        {
            return ObservationAcceptance::PayloadTooLarge;
        }
        if !recall_payload_is_bounded(&request.envelope.request, &observation.observation) {
            return ObservationAcceptance::PayloadTooLarge;
        }
        request.last_sequence_number = Some(observation.sequence_number);
        let is_stream_update = matches!(
            (&request.envelope.request, &observation.observation),
            (
                HostRequest::ObservePlayback { .. },
                HostObservation::PlaybackObserved { .. }
            )
        );
        if !is_stream_update {
            request.status = HostRequestStatus::Completed;
        }
        ObservationAcceptance::Accepted
    }
}

#[derive(Default)]
pub struct SubscriptionRegistry {
    next_value: u64,
    subscriptions: BTreeMap<SubscriptionId, ProjectionRequest>,
}

impl SubscriptionRegistry {
    #[must_use]
    pub fn subscribe(&mut self, request: ProjectionRequest) -> SubscriptionId {
        self.next_value = self
            .next_value
            .checked_add(1)
            .expect("subscription ID exhausted");
        let id = SubscriptionId::from_parts(0, self.next_value);
        self.subscriptions.insert(id, request);
        id
    }

    #[must_use]
    pub fn unsubscribe(&mut self, subscription_id: SubscriptionId) -> bool {
        self.subscriptions.remove(&subscription_id).is_some()
    }

    #[must_use]
    pub fn request(&self, subscription_id: SubscriptionId) -> Option<ProjectionRequest> {
        self.subscriptions.get(&subscription_id).copied()
    }
}
