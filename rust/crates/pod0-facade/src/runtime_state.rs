use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use pod0_application::{
    Clock, CommandEnvelope, CommandLedger, CommandRegistration, CoreFailure, CoreFailureCode,
    CoreWakeReason, HostCancellationRequest, HostObservation, HostRequestEnvelope,
    HostRequestLedger, OperationProjection, OperationResult, OperationStage, PlaybackPolicyState,
    SubscriptionRegistry,
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
use crate::runtime_agent_modules::state::{PendingAgentRecallObservation, PendingAgentRequest};
use crate::runtime_evidence_state::PendingEvidenceIndex;
pub(super) use crate::runtime_failure::failure;
use crate::runtime_feed_state::PendingFeed;
use crate::runtime_playback_state::PlaybackRuntime;
use crate::runtime_recall_cutover::PendingRecallCutover;
use crate::runtime_recall_interrupts::{RecallInterruptLease, RecallInterruptRegistry};
use crate::runtime_recall_state::{PendingRecall, RecallWorkflow};

pub(super) struct FacadeStores {
    pub(super) listening: LibraryStore,
    pub(super) evidence: EvidenceStore,
    pub(super) transcript: TranscriptStore,
    pub(super) scheduled_agent: Option<ScheduledAgentStore>,
    pub(super) agent: AgentStore,
    pub(super) publication: PublicationStore,
}

pub(super) struct FacadeState {
    pub(super) clock: Arc<dyn Clock>,
    pub(super) revision: StateRevision,
    pub(super) listening: ListeningDomainSnapshot,
    pub(super) notes: pod0_storage::NoteCollectionSnapshot,
    pub(super) clips: pod0_storage::ClipCollectionSnapshot,
    pub(super) store: Option<LibraryStore>,
    pub(super) evidence_store: Option<EvidenceStore>,
    pub(super) transcript_store: Option<TranscriptStore>,
    pub(super) scheduled_agent_store: Option<ScheduledAgentStore>,
    pub(super) agent_store: Option<AgentStore>,
    pub(super) publication_store: Option<PublicationStore>,
    pub(super) pending_publications: VecDeque<pod0_application::Pod0PublicationDraft>,
    pub(super) recall_index: RecallIndex,
    pub(super) recall_configuration: pod0_domain::RecallConfiguration,
    pub(super) recall_interrupts: Arc<RecallInterruptRegistry>,
    pub(super) commands: CommandLedger,
    pub(super) host_requests: HostRequestLedger,
    pub(super) host_queue: VecDeque<HostRequestEnvelope>,
    pub(super) host_cancellations: VecDeque<HostCancellationRequest>,
    pub(super) pending_feeds: BTreeMap<pod0_domain::HostRequestId, PendingFeed>,
    pub(super) pending_publisher_chapters:
        BTreeMap<HostRequestId, pod0_storage::PublisherChapterWorkflowRecord>,
    pub(super) pending_publisher_observations: BTreeMap<HostRequestId, HostObservation>,
    pub(super) pending_downloads: BTreeMap<HostRequestId, pod0_storage::DownloadHostRequestRecord>,
    pub(super) pending_download_observations:
        BTreeMap<HostRequestId, pod0_application::HostObservationEnvelope>,
    pub(super) pending_model_chapters: BTreeMap<HostRequestId, pod0_domain::EpisodeId>,
    pub(super) pending_model_observations:
        BTreeMap<HostRequestId, pod0_application::HostObservationEnvelope>,
    pub(super) pending_transcripts: BTreeMap<HostRequestId, EpisodeId>,
    pub(super) pending_transcript_observations:
        BTreeMap<HostRequestId, pod0_application::HostObservationEnvelope>,
    pub(super) pending_scheduled_agents:
        BTreeMap<HostRequestId, pod0_storage::ScheduledAgentHostRequestRecord>,
    pub(super) pending_scheduled_agent_observations:
        BTreeMap<HostRequestId, pod0_application::HostObservationEnvelope>,
    pub(super) pending_agents: BTreeMap<HostRequestId, PendingAgentRequest>,
    pub(super) pending_agent_observations:
        BTreeMap<HostRequestId, pod0_application::HostObservationEnvelope>,
    pub(super) pending_agent_recalls: BTreeMap<RecallQueryId, pod0_domain::AgentTurnId>,
    pub(super) pending_agent_recall_observations:
        BTreeMap<HostRequestId, PendingAgentRecallObservation>,
    pub(super) pending_core_wakes: BTreeMap<HostRequestId, CoreWakeReason>,
    pub(super) pending_evidence_indexes: BTreeMap<HostRequestId, PendingEvidenceIndex>,
    pub(super) pending_recall_cutovers: BTreeMap<HostRequestId, PendingRecallCutover>,
    pub(super) pending_recalls: BTreeMap<HostRequestId, PendingRecall>,
    pub(super) recalls: BTreeMap<RecallQueryId, RecallWorkflow>,
    pub(super) playback: PlaybackRuntime,
    pub(super) operations: Vec<OperationProjection>,
    pub(super) subscriptions: SubscriptionRegistry,
    pub(super) subscribers: BTreeMap<SubscriptionId, Arc<dyn ProjectionSubscriber>>,
}

impl FacadeState {
    #[cfg(test)]
    pub(super) fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            ..Self::default()
        }
    }

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

    pub(super) fn open(
        stores: FacadeStores,
        mut recall_index: RecallIndex,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, pod0_storage::StorageError> {
        let FacadeStores {
            listening: store,
            evidence: evidence_store,
            transcript: transcript_store,
            scheduled_agent: scheduled_agent_store,
            agent: agent_store,
            publication: publication_store,
        } = stores;
        let _ = store.clear_session_sleep_timer()?;
        let _ = store.recover_download_artifacts()?;
        let listening = store.snapshot()?;
        let notes = store.note_snapshot()?;
        let clips = store.clip_snapshot()?;
        let recall_configuration = store.recall_configuration()?.unwrap_or_default();
        recall_index
            .activate_embedding_space(recall_configuration.embedding_space_id)
            .map_err(|_| pod0_storage::StorageError::InvalidRecallConfiguration)?;
        let playback = PlaybackRuntime {
            policy_state: if listening.playback.active_episode_id.is_some() {
                PlaybackPolicyState::Paused
            } else {
                PlaybackPolicyState::Idle
            },
            ..PlaybackRuntime::default()
        };
        let mut state = Self {
            clock,
            revision: StateRevision::new(
                listening
                    .playback
                    .revision
                    .value
                    .max(notes.revision.value)
                    .max(clips.revision.value),
            ),
            listening,
            notes,
            clips,
            store: Some(store),
            evidence_store: Some(evidence_store),
            transcript_store: Some(transcript_store),
            scheduled_agent_store,
            agent_store: Some(agent_store),
            publication_store: Some(publication_store),
            recall_index,
            recall_configuration,
            playback,
            ..Self::default()
        };
        state.rehydrate_publisher_chapter_workflows()?;
        state.rehydrate_download_workflows()?;
        state.rehydrate_model_chapter_workflows()?;
        state.rehydrate_transcript_workflows()?;
        state.rehydrate_scheduled_agent_workflows()?;
        state.rehydrate_agent_turns()?;
        Ok(state)
    }

    pub(super) fn dispatch(&mut self, envelope: CommandEnvelope) -> bool {
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
