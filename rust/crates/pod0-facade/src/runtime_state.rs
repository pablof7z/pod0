use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use pod0_application::{
    Clock, CommandEnvelope, CommandLedger, CommandRegistration, CoreFailure, CoreFailureCode,
    HostRequestLedger, OperationProjection, OperationResult,
    OperationStage, Projection, SubscriptionRegistry,
};
use pod0_domain::{
    CommandId, EpisodeId, HostRequestId, ListeningDomainSnapshot, RecallQueryId, StateRevision,
    SubscriptionId,
};
use pod0_recall_index::RecallIndex;
use pod0_storage::{
    AgentStore, EvidenceStore, LibraryStore, PublicationStore, ScheduledAgentStore, TranscriptStore,
};

use crate::ProjectionSubscriber;
use crate::runtime_delivery_content::ProjectionDeliveryContent;
pub(super) use crate::runtime_failure::failure;
use crate::runtime_playback_state::PlaybackRuntime;
use crate::runtime_recall_interrupts::{RecallInterruptLease, RecallInterruptRegistry};
use crate::runtime_recall_state::RecallWorkflow;
use crate::user_data_erasure_facade::{ErasureLifecycle, PreparedFacadeErasure};

pub(super) struct FacadeState {
    pub(super) core_store_path: Option<PathBuf>,
    pub(super) erasure_lifecycle: ErasureLifecycle,
    pub(super) prepared_erasure: Option<PreparedFacadeErasure>,
    pub(super) clock: Arc<dyn Clock>,
    pub(super) revision: StateRevision,
    pub(super) listening: ListeningDomainSnapshot,
    pub(super) new_episode_notification_settings:
        pod0_application::NewEpisodeNotificationSettingsProjection,
    pub(super) notes: pod0_storage::NoteCollectionSnapshot,
    pub(super) memories: pod0_storage::MemoryCollectionSnapshot,
    pub(super) clips: pod0_storage::ClipCollectionSnapshot,
    pub(super) store: Option<LibraryStore>,
    pub(super) evidence_store: Option<EvidenceStore>,
    pub(super) transcript_store: Option<TranscriptStore>,
    pub(super) scheduled_agent_store: Option<ScheduledAgentStore>,
    pub(super) agent_store: Option<AgentStore>,
    pub(super) publication_store: Option<PublicationStore>,
    pub(super) recall_index: RecallIndex,
    pub(super) recall_configuration: pod0_domain::RecallConfiguration,
    pub(super) recall_interrupts: Arc<RecallInterruptRegistry>,
    pub(super) commands: CommandLedger,
    pub(super) host_requests: HostRequestLedger,
    pub(super) feed_fetches: Vec<pod0_storage::FeedFetchWorkflowRecord>,
    pub(super) pending_transcripts: BTreeMap<HostRequestId, EpisodeId>,
    pub(super) recalls: BTreeMap<RecallQueryId, RecallWorkflow>,
    pub(super) playback: PlaybackRuntime,
    pub(super) operations: Vec<OperationProjection>,
    pub(super) subscriptions: SubscriptionRegistry,
    pub(super) subscribers: BTreeMap<SubscriptionId, Arc<dyn ProjectionSubscriber>>,
    pub(super) delivered_projections: BTreeMap<SubscriptionId, Projection>,
    pub(super) delivered_contents: BTreeMap<SubscriptionId, ProjectionDeliveryContent>,
}

impl FacadeState {
    #[cfg(test)]
    pub(super) fn set_clock(&mut self, clock: Arc<dyn Clock>) {
        self.clock = clock;
    }

    pub(super) fn now(&self) -> pod0_domain::UnixTimestampMilliseconds {
        self.clock.now()
    }

    pub(super) fn begin_recall_index_operation(
        &self,
        cancellation_id: pod0_domain::CancellationId,
    ) -> RecallInterruptLease {
        self.recall_interrupts
            .begin(cancellation_id, self.recall_index.cancellation())
    }

    pub(super) fn dispatch(&mut self, envelope: CommandEnvelope) -> bool {
        if self.erasure_lifecycle != ErasureLifecycle::Active {
            return false;
        }
        match self.commands.register(envelope.clone(), self.revision) {
            CommandRegistration::Accepted => self.accept_command(envelope),
            CommandRegistration::StaleRevision => {
                self.advance_revision();
                self.operations.push(OperationProjection {
                    command_id: envelope.command_id,
                    cancellation_id: envelope.cancellation_id,
                    stage: OperationStage::Failed,
                    failure: Some(failure(CoreFailureCode::RevisionConflict)),
                    result: None,
                });
                self.trim_operations();
                true
            }
            CommandRegistration::Duplicate | CommandRegistration::ConflictingReuse => false,
        }
    }

    pub(super) fn advance_revision(&mut self) {
        self.revision = StateRevision::new(
            self.revision
                .value
                .checked_add(1)
                .expect("state revision exhausted"),
        );
    }

    pub(super) fn begin(&mut self, envelope: &CommandEnvelope) {
        self.advance_revision();
        self.operations.push(OperationProjection {
            command_id: envelope.command_id,
            cancellation_id: envelope.cancellation_id,
            stage: OperationStage::Accepted,
            failure: None,
            result: None,
        });
    }

    pub(super) fn fail(&mut self, command_id: CommandId, code: CoreFailureCode) {
        self.finish(
            command_id,
            OperationStage::Failed,
            Some(failure(code)),
            None,
        );
    }

    pub(super) fn succeed(&mut self, command_id: CommandId, result: Option<OperationResult>) {
        self.finish(command_id, OperationStage::Succeeded, None, result);
    }

    pub(super) fn finish(
        &mut self,
        command_id: CommandId,
        stage: OperationStage,
        operation_failure: Option<CoreFailure>,
        result: Option<OperationResult>,
    ) {
        if let Some(operation) = self
            .operations
            .iter_mut()
            .rev()
            .find(|operation| operation.command_id == command_id)
        {
            operation.stage = stage;
            operation.failure = operation_failure;
            operation.result = result;
        }
    }

    pub(super) fn reload_listening(&mut self) -> Result<(), pod0_storage::StorageError> {
        if let Some(store) = &self.store {
            let listening = store.snapshot()?;
            self.revision =
                StateRevision::new(self.revision.value.max(listening.playback.revision.value));
            self.listening = listening;
        }
        Ok(())
    }

    pub(super) fn trim_operations(&mut self) {
        if self.operations.len() > pod0_application::MAX_OPERATION_ITEMS {
            let excess = self.operations.len() - pod0_application::MAX_OPERATION_ITEMS;
            self.operations.drain(..excess);
        }
    }
}
