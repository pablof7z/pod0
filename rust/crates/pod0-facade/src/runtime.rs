use std::sync::{Arc, Mutex, MutexGuard};

use pod0_application::{
    ApplicationCommand, CommandEnvelope, ProjectionEnvelope, ProjectionRequest,
};
use pod0_domain::{CancellationId, CommandId, SubscriptionId};
use pod0_recall_index::{RECALL_INDEX_DIMENSIONS, RecallIndex, recall_index_path_for_core_store};
use pod0_storage::{EvidenceStore, LibraryStore, TranscriptStore};
use sha2::{Digest as _, Sha256};
use std::path::Path;

use crate::ProjectionSubscriber;
use crate::runtime_clock::SystemClock;
use crate::runtime_open_error::FacadeOpenError;
use crate::runtime_recall_interrupts::RecallInterruptRegistry;
use crate::runtime_state::FacadeState;
use crate::runtime_state_open::FacadeStores;

mod api;
mod public_api;

#[derive(uniffi::Object)]
pub struct Pod0Facade {
    pub(super) state: Arc<Mutex<FacadeState>>,
    pub(super) recall_interrupts: Arc<RecallInterruptRegistry>,
}
impl Pod0Facade {
    fn from_state(state: FacadeState) -> Arc<Self> {
        let recall_interrupts = Arc::clone(&state.recall_interrupts);
        Arc::new(Self {
            state: Arc::new(Mutex::new(state)),
            recall_interrupts,
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
        if let Some(parent) = path.parent()
            && !pod0_storage::pending_user_data_erasure_markers(parent)
                .map_err(FacadeOpenError::from)?
                .is_empty()
        {
            return Err(FacadeOpenError::ErasureRecoveryRequired);
        }
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
            },
            recall_index,
            clock,
        )
        .map_err(FacadeOpenError::from)?;
        Ok(Self::from_state(state))
    }

    pub(super) fn notify_subscribers(&self) {
        let deliveries = self.state().deliveries();
        for (subscriber, projection) in deliveries {
            subscriber.receive(projection);
        }
    }

    /// Returns durable pending host work without claiming leases or changing
    /// retry state. Intended for diagnostics and headless host inspection.
    pub fn pending_host_effects(
        &self,
        maximum_count: u16,
    ) -> Result<Vec<pod0_application::DurableExternalEffectRequest>, FacadeOpenError> {
        let store = self
            .state()
            .store
            .clone()
            .ok_or(FacadeOpenError::StorageUnavailable)?;
        store
            .pending_effect_requests(maximum_count)
            .map_err(|_| FacadeOpenError::StorageUnavailable)
    }

    /// Returns the authoritative time at which durable host work can next be
    /// claimed, including delayed core wakes and expired-lease recovery.
    pub fn next_host_effect_at(
        &self,
    ) -> Result<Option<pod0_domain::UnixTimestampMilliseconds>, FacadeOpenError> {
        let state = self.state();
        let store = state
            .store
            .clone()
            .ok_or(FacadeOpenError::StorageUnavailable)?;
        store
            .next_effect_claim_at(state.now())
            .map_err(|_| FacadeOpenError::StorageUnavailable)
    }

    pub fn next_leased_headless_host_requests(
        &self,
        maximum_count: u16,
    ) -> Vec<pod0_application::LeasedHostRequestEnvelope> {
        let (changed, requests) = self.state().next_leased_headless_requests(maximum_count);
        if changed {
            self.notify_subscribers();
        }
        requests
    }

    pub fn library_page_with_totals(
        &self,
        offset: u32,
        max_items: u16,
    ) -> (
        ProjectionEnvelope,
        (usize, usize, usize),
        Vec<pod0_domain::PodcastId>,
    ) {
        let state = self.state();
        let totals = (
            state.listening.podcasts.len(),
            state.listening.subscriptions.len(),
            state.listening.episodes.len(),
        );
        let projection = state.snapshot(ProjectionRequest {
            scope: pod0_application::ProjectionScope::Library,
            offset,
            max_items,
        });
        let subscribed_podcast_ids = match &projection.projection {
            pod0_application::Projection::Library { value } => value
                .podcasts
                .iter()
                .filter(|podcast| {
                    state
                        .listening
                        .subscriptions
                        .iter()
                        .any(|subscription| subscription.podcast_id == podcast.podcast_id)
                })
                .map(|podcast| podcast.podcast_id)
                .collect(),
            _ => Vec::new(),
        };
        (projection, totals, subscribed_podcast_ids)
    }
}
