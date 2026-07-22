use std::collections::BTreeMap;

use pod0_domain::{CancellationId, CommandId, HostRequestId, StateRevision};

use crate::contract_state_download_validation::download_payload_is_bounded;
use crate::contract_state_validation::{
    chapter_model_payload_is_bounded, observation_matches_request, recall_payload_is_bounded,
};
use crate::{
    CommandEnvelope, HostObservation, HostObservationEnvelope, HostRequest, HostRequestEnvelope,
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
    pub fn is_chapter_model_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::ExecuteChapterModel { .. }
                    | HostRequest::RecoverChapterModelOperation { .. }
            )
        })
    }

    #[must_use]
    pub fn is_transcript_request(&self, request_id: HostRequestId) -> bool {
        self.requests.get(&request_id).is_some_and(|request| {
            matches!(
                request.envelope.request,
                HostRequest::ExecuteTranscriptCapability { .. }
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

    #[must_use]
    pub fn matches_outstanding(&self, request: &HostRequestEnvelope) -> bool {
        self.requests
            .get(&request.request_id)
            .is_some_and(|tracked| {
                tracked.status == HostRequestStatus::Outstanding && tracked.envelope == *request
            })
    }

    pub fn cancel_request(&mut self, request_id: HostRequestId) -> bool {
        let Some(request) = self.requests.get_mut(&request_id) else {
            return false;
        };
        if request.status != HostRequestStatus::Outstanding {
            return false;
        }
        request.status = HostRequestStatus::Cancelled;
        true
    }

    pub fn retire(&mut self, request_id: HostRequestId) -> bool {
        self.requests.remove(&request_id).is_some()
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
        if !chapter_model_payload_is_bounded(&request.envelope.request, &observation.observation) {
            return ObservationAcceptance::PayloadTooLarge;
        }
        if !download_payload_is_bounded(&request.envelope.request, &observation.observation) {
            return ObservationAcceptance::PayloadTooLarge;
        }
        request.last_sequence_number = Some(observation.sequence_number);
        let is_stream_update = matches!(
            (&request.envelope.request, &observation.observation),
            (
                HostRequest::ObservePlayback { .. },
                HostObservation::PlaybackObserved { .. }
            ) | (
                HostRequest::ExecuteChapterModel { .. },
                HostObservation::ChapterModelProviderAccepted { .. }
            ) | (
                HostRequest::RecoverChapterModelOperation { .. },
                HostObservation::ChapterModelProviderAccepted { .. }
            ) | (
                HostRequest::StartEpisodeDownload { .. },
                HostObservation::DownloadAccepted { .. }
            ) | (
                HostRequest::ExecuteScheduledAgentTurn { .. },
                HostObservation::ScheduledAgentExecutionObserved {
                    observation: crate::ScheduledAgentExecutionObservation::Accepted { .. }
                }
            )
        );
        if !is_stream_update {
            request.status = HostRequestStatus::Completed;
        }
        ObservationAcceptance::Accepted
    }
}
