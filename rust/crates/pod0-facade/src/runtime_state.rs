use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use pod0_application::{
    ApplicationCommand, CommandEnvelope, CommandLedger, CommandRegistration, CoreFailure,
    CoreFailureCode, HostFailureCode, HostObservation, HostObservationEnvelope, HostRequest,
    HostRequestEnvelope, HostRequestLedger, LibraryProjection, MAX_FEED_RESPONSE_BYTES,
    MAX_OPERATION_ITEMS, ObservationAcceptance, OperationProjection, OperationStage,
    PlaybackProjection, Projection, ProjectionEnvelope, ProjectionRequest, ProjectionScope,
    Retryability, SubscriptionRegistry, UnsupportedProjection, UserAction,
};
use pod0_domain::{CommandId, HostRequestId, StateRevision, SubscriptionId};

use crate::ProjectionSubscriber;

#[derive(Default)]
pub(super) struct FacadeState {
    revision: StateRevision,
    commands: CommandLedger,
    host_requests: HostRequestLedger,
    pub(super) host_queue: VecDeque<HostRequestEnvelope>,
    operations: Vec<OperationProjection>,
    pub(super) subscriptions: SubscriptionRegistry,
    pub(super) subscribers: BTreeMap<SubscriptionId, Arc<dyn ProjectionSubscriber>>,
}

impl FacadeState {
    pub(super) fn dispatch(&mut self, envelope: CommandEnvelope) -> bool {
        match self.commands.register(envelope.clone(), self.revision) {
            CommandRegistration::Accepted => self.accept_command(envelope),
            CommandRegistration::StaleRevision => self.reject_stale_command(envelope),
            CommandRegistration::Duplicate | CommandRegistration::ConflictingReuse => false,
        }
    }

    fn accept_command(&mut self, envelope: CommandEnvelope) -> bool {
        self.advance_revision();
        self.operations.push(OperationProjection {
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            stage: OperationStage::Accepted,
            failure: None,
        });
        match envelope.command {
            ApplicationCommand::SubscribeToFeed { feed_url } => {
                let request = HostRequestEnvelope {
                    request_id: HostRequestId::from_parts(
                        envelope.command_id.high,
                        envelope.command_id.low,
                    ),
                    command_id: envelope.command_id,
                    cancellation_id: envelope.cancellation_id,
                    issued_revision: self.revision,
                    deadline_at: None,
                    request: HostRequest::FetchFeed {
                        feed_url,
                        entity_tag: None,
                        last_modified: None,
                        maximum_response_bytes: MAX_FEED_RESPONSE_BYTES,
                    },
                };
                if self.host_requests.register(request.clone()) {
                    self.host_queue.push_back(request);
                    self.finish(envelope.command_id, OperationStage::Running, None);
                } else {
                    self.fail(envelope.command_id, CoreFailureCode::InvalidCommand);
                }
            }
            ApplicationCommand::CancelOperation { cancellation_id } => {
                self.host_requests.cancel(cancellation_id);
                self.host_queue
                    .retain(|request| request.cancellation_id != cancellation_id);
                for operation in &mut self.operations {
                    if operation.cancellation_id == cancellation_id
                        && !operation.stage.is_terminal()
                    {
                        operation.stage = OperationStage::Cancelled;
                        operation.failure = Some(failure(CoreFailureCode::Cancelled));
                    }
                }
                self.finish(envelope.command_id, OperationStage::Succeeded, None);
            }
            ApplicationCommand::Unsubscribe { .. } | ApplicationCommand::RequestPlayback { .. } => {
                self.fail(envelope.command_id, CoreFailureCode::NotFound);
            }
            ApplicationCommand::Unsupported { wire_code } => {
                self.fail(
                    envelope.command_id,
                    CoreFailureCode::Unsupported { wire_code },
                );
            }
        }
        self.operations.truncate_from_start(MAX_OPERATION_ITEMS);
        true
    }

    fn reject_stale_command(&mut self, envelope: CommandEnvelope) -> bool {
        self.advance_revision();
        self.operations.push(OperationProjection {
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            stage: OperationStage::Failed,
            failure: Some(failure(CoreFailureCode::RevisionConflict)),
        });
        self.operations.truncate_from_start(MAX_OPERATION_ITEMS);
        true
    }

    pub(super) fn record_host_observation(&mut self, observation: HostObservationEnvelope) -> bool {
        let command_id = self.host_requests.command_id(observation.request_id);
        let is_playback_stream = self
            .host_requests
            .is_playback_observation_stream(observation.request_id);
        if self.host_requests.accept_observation(&observation) != ObservationAcceptance::Accepted {
            return false;
        }
        let Some(command_id) = command_id else {
            return false;
        };
        self.advance_revision();
        match observation.observation {
            HostObservation::Failed { code, .. } => {
                let failure_code = match code {
                    HostFailureCode::PermissionDenied => CoreFailureCode::HostRejected,
                    _ => CoreFailureCode::HostUnavailable,
                };
                self.fail(command_id, failure_code);
            }
            HostObservation::Cancelled => self.finish(
                command_id,
                OperationStage::Cancelled,
                Some(failure(CoreFailureCode::Cancelled)),
            ),
            HostObservation::FeedBytesFetched { .. } | HostObservation::FeedNotModified { .. } => {
                self.fail(command_id, CoreFailureCode::Unsupported { wire_code: 1 });
            }
            HostObservation::PlaybackObserved { .. } if is_playback_stream => {
                self.finish(command_id, OperationStage::Running, None);
            }
            HostObservation::PlaybackObserved { .. } => {
                self.finish(command_id, OperationStage::Succeeded, None);
            }
            HostObservation::Unsupported { wire_code } => {
                self.fail(command_id, CoreFailureCode::Unsupported { wire_code });
            }
        }
        true
    }

    fn advance_revision(&mut self) {
        self.revision = StateRevision::new(
            self.revision
                .value
                .checked_add(1)
                .expect("state revision exhausted"),
        );
    }

    fn fail(&mut self, command_id: CommandId, code: CoreFailureCode) {
        self.finish(command_id, OperationStage::Failed, Some(failure(code)));
    }

    fn finish(
        &mut self,
        command_id: CommandId,
        stage: OperationStage,
        operation_failure: Option<CoreFailure>,
    ) {
        if let Some(operation) = self
            .operations
            .iter_mut()
            .rev()
            .find(|operation| operation.command_id == command_id)
        {
            operation.stage = stage;
            operation.failure = operation_failure;
        }
    }

    pub(super) fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope {
        let item_limit = request.bounded_max_items();
        let projection = match request.scope {
            ProjectionScope::Library => {
                let mut value = LibraryProjection {
                    podcasts: Vec::new(),
                    episodes: Vec::new(),
                    operations: self.operations.clone(),
                    has_more: false,
                };
                value.enforce_bounds(item_limit);
                Projection::Library { value }
            }
            ProjectionScope::Playback => {
                let mut value = PlaybackProjection {
                    current: None,
                    queue: Vec::new(),
                    operations: self.operations.clone(),
                };
                value.enforce_bounds(item_limit);
                Projection::Playback { value }
            }
            ProjectionScope::Unsupported { wire_code } => Projection::Unsupported {
                value: UnsupportedProjection {
                    wire_code,
                    message: "unsupported projection scope".to_owned(),
                },
            },
        };
        ProjectionEnvelope {
            contract_version: pod0_application::FACADE_CONTRACT_VERSION,
            state_revision: self.revision,
            projection,
        }
    }

    pub(super) fn deliveries(&self) -> Vec<(Arc<dyn ProjectionSubscriber>, ProjectionEnvelope)> {
        self.subscribers
            .iter()
            .filter_map(|(id, subscriber)| {
                self.subscriptions
                    .request(*id)
                    .map(|request| (Arc::clone(subscriber), self.snapshot(request)))
            })
            .collect()
    }
}

fn failure(code: CoreFailureCode) -> CoreFailure {
    CoreFailure {
        code,
        safe_detail: None,
        retryability: Retryability::Never,
        user_action: UserAction::None,
    }
}

trait TruncateFromStart<T> {
    fn truncate_from_start(&mut self, maximum_count: usize);
}

impl<T> TruncateFromStart<T> for Vec<T> {
    fn truncate_from_start(&mut self, maximum_count: usize) {
        if self.len() > maximum_count {
            self.drain(..self.len() - maximum_count);
        }
    }
}
