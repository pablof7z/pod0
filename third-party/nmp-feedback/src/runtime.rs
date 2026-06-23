use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use nmp_ffi::NmpApp;

use crate::command::{fetch_feedback, publish_feedback, FeedbackCommandOutcome};
use crate::config::FeedbackConfig;
use crate::observer::{FeedbackEventCache, FeedbackObserver};
use crate::projection::{reduce_feedback_threads, FeedbackThreadDto};

pub type SnapshotBump = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Clone)]
pub struct FeedbackRuntime {
    config: FeedbackConfig,
    events: FeedbackEventCache,
    rev: Arc<AtomicU64>,
    snapshot_bump: Option<SnapshotBump>,
}

impl FeedbackRuntime {
    #[must_use]
    pub fn new(config: FeedbackConfig, events: FeedbackEventCache, rev: Arc<AtomicU64>) -> Self {
        Self {
            config,
            events,
            rev,
            snapshot_bump: None,
        }
    }

    #[must_use]
    pub fn with_snapshot_bump(mut self, bump: SnapshotBump) -> Self {
        self.snapshot_bump = Some(bump);
        self
    }

    #[must_use]
    pub fn config(&self) -> &FeedbackConfig {
        &self.config
    }

    #[must_use]
    pub fn events(&self) -> FeedbackEventCache {
        self.events.clone()
    }

    #[must_use]
    pub fn observer(&self) -> FeedbackObserver {
        let observer =
            FeedbackObserver::new(self.config.clone(), self.events.clone(), self.rev.clone());
        match &self.snapshot_bump {
            Some(bump) => observer.with_snapshot_bump(bump.clone()),
            None => observer,
        }
    }

    #[must_use]
    pub fn snapshot_events(&self) -> Vec<serde_json::Value> {
        self.events
            .lock()
            .ok()
            .map(|events| events.clone())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn snapshot_threads(&self) -> Vec<FeedbackThreadDto> {
        reduce_feedback_threads(&self.snapshot_events(), &self.config.project_coordinate)
    }

    #[must_use]
    pub fn fetch(&self, app: *mut NmpApp) -> FeedbackCommandOutcome {
        fetch_feedback(app, &self.config)
    }

    #[must_use]
    pub fn publish(
        &self,
        app: *mut NmpApp,
        category: &str,
        content: &str,
        parent_event_id: Option<&str>,
        reply_to_pubkey: Option<&str>,
    ) -> FeedbackCommandOutcome {
        publish_feedback(
            app,
            &self.config,
            category,
            content,
            parent_event_id,
            reply_to_pubkey,
        )
    }
}
