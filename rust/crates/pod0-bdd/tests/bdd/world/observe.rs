//! The observation plane: everything a `Then` step is allowed to read.
//!
//! Four admissible observables, all on the public facade surface:
//! projection SNAPSHOTS (`snapshot`), event-driven DELIVERIES to a live
//! subscriber (`subscribe`/`unsubscribe`), drained HOST WORK
//! (`next_host_requests`/`next_host_cancellations`), and the typed RECEIPT
//! `record_host_observation` returned. Nothing here opens the database or
//! peeks at facade internals — if a fact is not visible through those four,
//! a scenario may not claim it.

use std::sync::{Arc, Mutex};

use pod0_application::{
    FeedFetchProjection, HostCancellationRequest, HostRequest, HostRequestEnvelope,
    LibraryProjection, OperationProjection, Projection, ProjectionEnvelope, ProjectionRequest,
    ProjectionScope,
};
use pod0_domain::StateRevision;
use pod0_facade::ProjectionSubscriber;

use super::{LibraryWatch, PodWorld};

/// The one bounded library request every observation uses, so every claim
/// about "the library" reads the same page the app's first render would.
pub(super) fn library_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Library,
        offset: 0,
        max_items: 50,
    }
}

/// A live subscriber that records every delivery, exactly as a rendering
/// screen would receive them.
#[derive(Default)]
pub(super) struct RecordingSubscriber {
    deliveries: Mutex<Vec<ProjectionEnvelope>>,
}

impl RecordingSubscriber {
    pub(super) fn count(&self) -> usize {
        self.lock().len()
    }

    pub(super) fn library_deliveries(&self) -> Vec<LibraryProjection> {
        self.lock()
            .iter()
            .filter_map(|envelope| match &envelope.projection {
                Projection::Library { value } => Some(value.clone()),
                _ => None,
            })
            .collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<ProjectionEnvelope>> {
        self.deliveries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl ProjectionSubscriber for RecordingSubscriber {
    fn receive(&self, projection: ProjectionEnvelope) {
        self.lock().push(projection);
    }
}

impl PodWorld {
    /// The current bounded library projection — what the app would render.
    pub fn library(&self) -> LibraryProjection {
        let envelope = self.facade().snapshot(library_request());
        let Projection::Library { value } = envelope.projection else {
            panic!("pod0-bdd: a library request must yield a library projection")
        };
        value
    }

    /// The current state revision, read off the same snapshot the app sees.
    pub fn revision(&self) -> StateRevision {
        self.facade().snapshot(library_request()).state_revision
    }

    /// Whether anything is running at all — the guard `nothing_to_observe!`
    /// checks before a negative claim reads an empty world.
    pub fn is_started(&self) -> bool {
        self.facade.is_some()
    }

    pub fn has_subscribed_to(&self, url: &str) -> bool {
        self.subscribes.contains_key(url)
    }

    /// The projected operation for the subscribe the scenario issued against
    /// `url`, if the projection carries one.
    pub fn subscribe_operation(&self, url: &str) -> Option<OperationProjection> {
        let command_id = self.subscribes.get(url)?.envelope.command_id;
        self.library()
            .operations
            .into_iter()
            .find(|operation| operation.command_id == command_id)
    }

    /// The durable fetch workflow for `url`, if one remains projected.
    pub fn feed_fetch(&self, url: &str) -> Option<FeedFetchProjection> {
        let identity = pod0_application::normalize_feed_url(url)?;
        self.library().feed_fetches.into_iter().find(|fetch| {
            pod0_application::normalize_feed_url(&fetch.feed_url)
                .is_some_and(|candidate| candidate.comparison_key == identity.comparison_key)
        })
    }

    /// Drain the host-work queue to exhaustion, as a host draining until
    /// empty would. Bounded batches are the contract; the loop is the host's.
    pub fn drain_all_host_requests(&self) -> Vec<HostRequestEnvelope> {
        let facade = self.facade();
        let mut drained = Vec::new();
        loop {
            let batch = facade.next_host_requests(16);
            if batch.is_empty() {
                return drained;
            }
            drained.extend(batch);
        }
    }

    pub fn drain_host_cancellations(&self) -> Vec<HostCancellationRequest> {
        self.facade().next_host_cancellations(16)
    }

    /// The accepted-but-undelivered announcement, for claims about work the
    /// host is already holding.
    pub fn accepted_announcement(&self) -> Option<&HostRequestEnvelope> {
        self.accepted_announcement.as_ref()
    }

    /// Resolve a podcast title the scenario names to the identity the
    /// contract speaks — through the projection, exactly as the app would.
    pub fn podcast_id_by_title(&mut self, title: &str) -> pod0_domain::PodcastId {
        self.ensure_started();
        self.library()
            .podcasts
            .into_iter()
            .find(|podcast| podcast.title == title)
            .unwrap_or_else(|| {
                panic!("pod0-bdd: the library lists no podcast titled {title:?} to act on")
            })
            .podcast_id
    }

    pub fn last_receipt(&self) -> Option<&pod0_application::HostObservationReceipt> {
        self.last_receipt.as_ref()
    }

    pub fn revision_at_cancel(&self) -> Option<StateRevision> {
        self.revision_at_cancel
    }

    /// Count the feed fetches for `url` in a drained batch.
    pub fn feed_fetches_for(requests: &[HostRequestEnvelope], url: &str) -> usize {
        requests
            .iter()
            .filter(|request| {
                matches!(&request.request, HostRequest::FetchFeed { feed_url, .. } if feed_url == url)
            })
            .count()
    }

    // ---- the live library view -----------------------------------------

    /// `When the app opens a live library view` — a real `subscribe` with a
    /// recording subscriber; the immediate baseline delivery lands before
    /// this returns, per the contract.
    pub fn open_library_watch(&mut self) {
        self.ensure_started();
        assert!(
            self.library_watch.is_none(),
            "pod0-bdd: the scenario opened a second live library view; one is the catalog's shape"
        );
        let subscriber = Arc::new(RecordingSubscriber::default());
        let subscription_id = self
            .facade()
            .subscribe(library_request(), subscriber.clone());
        self.library_watch = Some(LibraryWatch {
            subscription_id,
            subscriber,
        });
    }

    /// `When the app closes the live library view` — a real `unsubscribe`,
    /// noting how many deliveries had arrived so a later `Then` can prove
    /// the count never moves again.
    pub fn close_library_watch(&mut self) {
        let watch = self
            .library_watch
            .as_ref()
            .expect("pod0-bdd: no live library view is open to close");
        self.facade().unsubscribe(watch.subscription_id);
        self.deliveries_at_close = Some(watch.subscriber.count());
    }

    pub fn library_watch_deliveries(&self) -> Option<Vec<LibraryProjection>> {
        self.library_watch
            .as_ref()
            .map(|watch| watch.subscriber.library_deliveries())
    }

    pub fn watch_delivery_count(&self) -> Option<usize> {
        self.library_watch
            .as_ref()
            .map(|watch| watch.subscriber.count())
    }

    pub fn deliveries_at_close(&self) -> Option<usize> {
        self.deliveries_at_close
    }
}
