use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use pod0_application::{
    ApplicationCommand, CommandEnvelope, HostCancellationRequest, HostObservationEnvelope,
    HostRequestEnvelope, ProjectionEnvelope, ProjectionRequest, bounded_host_request_count,
};
use pod0_domain::{CancellationId, SubscriptionId};
use pod0_recall_index::{RECALL_INDEX_DIMENSIONS, RecallIndex, recall_index_path_for_core_store};
use pod0_storage::{EvidenceStore, LibraryStore, TranscriptStore};
use std::path::Path;

use crate::ProjectionSubscriber;
use crate::runtime_clock::SystemClock;
use crate::runtime_recall_interrupts::RecallInterruptRegistry;
use crate::runtime_state::{FacadeState, FacadeStores};

mod api;

#[derive(uniffi::Object)]
pub struct Pod0Facade {
    pub(super) state: Arc<Mutex<FacadeState>>,
    pub(super) recall_interrupts: Arc<RecallInterruptRegistry>,
    pub(super) nmp_store_path: Option<String>,
    pub(super) nmp: Mutex<Option<pod0_nmp::NmpRuntime>>,
    pub(super) nmp_dispatcher: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug, uniffi::Error)]
pub enum FacadeOpenError {
    NotAuthoritative,
    SchemaBlocked,
    StorageUnavailable,
}

impl std::fmt::Display for FacadeOpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotAuthoritative => "shared listening store is not authoritative",
            Self::SchemaBlocked => "shared listening store schema is blocked",
            Self::StorageUnavailable => "shared listening store is unavailable",
        })
    }
}

impl std::error::Error for FacadeOpenError {}

impl Pod0Facade {
    #[cfg(test)]
    pub(super) fn with_clock(clock: Arc<dyn pod0_application::Clock>) -> Arc<Self> {
        Self::from_state(FacadeState::with_clock(clock), None)
    }

    fn from_state(state: FacadeState, nmp_store_path: Option<String>) -> Arc<Self> {
        let recall_interrupts = Arc::clone(&state.recall_interrupts);
        Arc::new(Self {
            state: Arc::new(Mutex::new(state)),
            recall_interrupts,
            nmp_store_path,
            nmp: Mutex::new(None),
            nmp_dispatcher: Mutex::new(None),
        })
    }

    pub(super) fn state(&self) -> MutexGuard<'_, FacadeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    pub(super) fn open_with_clock(
        store_path: String,
        clock: Arc<dyn pod0_application::Clock>,
    ) -> Arc<Self> {
        Self::open_with_clock_value(store_path, clock).expect("test store must open")
    }

    fn open_with_clock_value(
        store_path: String,
        clock: Arc<dyn pod0_application::Clock>,
    ) -> Result<Arc<Self>, FacadeOpenError> {
        let path = Path::new(&store_path);
        let store = LibraryStore::open_authoritative(path).map_err(FacadeOpenError::from)?;
        if !pod0_storage::chapter_store_is_authoritative(path).map_err(FacadeOpenError::from)? {
            return Err(FacadeOpenError::NotAuthoritative);
        }
        store
            .require_notes_authoritative()
            .map_err(FacadeOpenError::from)?;
        let evidence_store = EvidenceStore::open(path).map_err(FacadeOpenError::from)?;
        let transcript_store =
            TranscriptStore::open_authoritative(path).map_err(FacadeOpenError::from)?;
        let scheduled_agent_store = pod0_storage::scheduled_agent_store_is_authoritative(path)
            .map_err(FacadeOpenError::from)?
            .then(|| pod0_storage::ScheduledAgentStore::open_authoritative(path))
            .transpose()
            .map_err(FacadeOpenError::from)?;
        let agent_store = pod0_storage::AgentStore::open(path).map_err(FacadeOpenError::from)?;
        let publication_store =
            pod0_storage::PublicationStore::open(path).map_err(FacadeOpenError::from)?;
        let signer_store = pod0_storage::SignerStore::open(path).map_err(FacadeOpenError::from)?;
        let recall_index = RecallIndex::open(
            &recall_index_path_for_core_store(path),
            RECALL_INDEX_DIMENSIONS,
        )
        .map_err(FacadeOpenError::from)?;
        let state = FacadeState::open(
            FacadeStores {
                listening: store,
                evidence: evidence_store,
                transcript: transcript_store,
                scheduled_agent: scheduled_agent_store,
                agent: agent_store,
                publication: publication_store,
                signer: signer_store,
            },
            recall_index,
            clock,
        )
        .map_err(FacadeOpenError::from)?;
        let facade = Self::from_state(state, Some(format!("{}.nmp.redb", path.to_string_lossy())));
        if !facade.state().publication_records().is_empty() {
            facade.start_nmp()?;
            facade.recover_nmp_publications();
        }
        Ok(facade)
    }

    pub(super) fn notify_subscribers(&self) {
        let deliveries = self.state().deliveries();
        for (subscriber, projection) in deliveries {
            subscriber.receive(projection);
        }
    }
}

#[uniffi::export]
impl Pod0Facade {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::from_state(FacadeState::default(), None)
    }

    #[uniffi::constructor]
    pub fn open(store_path: String) -> Result<Arc<Self>, FacadeOpenError> {
        Self::open_with_clock_value(store_path, Arc::new(SystemClock))
    }

    pub fn dispatch(&self, command: CommandEnvelope) {
        let signer_sign_out = match command.command {
            ApplicationCommand::SignOutNostrSigner {
                expected_account_id,
            } => Some(expected_account_id),
            _ => None,
        };
        let cancellation_id = cancellation_target(&command);
        if let Some(cancellation_id) = cancellation_id {
            self.recall_interrupts.signal(cancellation_id);
        }
        let changed = self.state().dispatch(command);
        if let Some(cancellation_id) = cancellation_id {
            self.recall_interrupts.finish_signal(cancellation_id);
        }
        self.drive_pending_publications();
        if let Some(account_id) = signer_sign_out {
            self.detach_native_signer(account_id);
        }
        if changed {
            self.notify_subscribers();
        }
    }

    pub fn snapshot(&self, request: ProjectionRequest) -> ProjectionEnvelope {
        self.state().snapshot(request)
    }

    /// Plans the exact bounded chapter-model capability request from the
    /// authoritative Rust episode, transcript, and chapter selections.
    pub fn plan_chapter_model_request(
        &self,
        episode_id: pod0_domain::EpisodeId,
        configured_model: String,
    ) -> pod0_application::ChapterModelPlan {
        self.chapter_model_plan(episode_id, configured_model)
    }

    pub fn subscribe(
        &self,
        request: ProjectionRequest,
        subscriber: Arc<dyn ProjectionSubscriber>,
    ) -> SubscriptionId {
        let (subscription_id, projection) = {
            let mut state = self.state();
            let id = state.subscriptions.subscribe(request);
            state.subscribers.insert(id, Arc::clone(&subscriber));
            (id, state.snapshot(request))
        };
        subscriber.receive(projection);
        subscription_id
    }

    pub fn unsubscribe(&self, subscription_id: SubscriptionId) {
        let mut state = self.state();
        let _ = state.subscriptions.unsubscribe(subscription_id);
        state.subscribers.remove(&subscription_id);
    }

    pub fn next_host_requests(&self, maximum_count: u16) -> Vec<HostRequestEnvelope> {
        let (changed, requests) = {
            let mut state = self.state();
            let mut changed = state.retry_pending_publisher_observations();
            changed |= state.reconcile_download_deadlines();
            let _ = state.admit_publisher_chapter_requests();
            let _ = state.admit_download_requests();
            let _ = state.admit_scheduled_agent_requests();
            let maximum = bounded_host_request_count(maximum_count);
            let first_count = maximum.min(state.host_queue.len());
            let mut requests = state.host_queue.drain(..first_count).collect::<Vec<_>>();
            if requests.len() < maximum && state.prepare_model_chapter_host_request() {
                changed = true;
            }
            if requests.len() < maximum && state.prepare_transcript_host_request() {
                changed = true;
            }
            let remaining = maximum
                .saturating_sub(requests.len())
                .min(state.host_queue.len());
            requests.extend(state.host_queue.drain(..remaining));
            (changed, requests)
        };
        if changed {
            self.notify_subscribers();
        }
        requests
    }

    pub fn next_host_cancellations(&self, maximum_count: u16) -> Vec<HostCancellationRequest> {
        let mut state = self.state();
        let count = bounded_host_request_count(maximum_count).min(state.host_cancellations.len());
        state.host_cancellations.drain(..count).collect()
    }

    pub fn record_host_observation(
        &self,
        observation: HostObservationEnvelope,
    ) -> pod0_application::HostObservationReceipt {
        let signer_result = {
            let mut state = self.state();
            state.record_signer_observation(observation.clone())
        };
        if let Some(result) = signer_result {
            let changed = result.changed | self.apply_signer_runtime_action(result.action);
            if changed {
                self.notify_subscribers();
            }
            return result.receipt;
        }
        let (changed, receipt) = self.state().record_host_observation(observation);
        if changed {
            self.notify_subscribers();
        }
        receipt
    }
}

fn cancellation_target(command: &CommandEnvelope) -> Option<CancellationId> {
    match command.command {
        ApplicationCommand::CancelOperation { cancellation_id } => Some(cancellation_id),
        _ => None,
    }
}
