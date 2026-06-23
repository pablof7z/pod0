use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nmp_core::substrate::KernelEvent;
use nmp_core::KernelEventObserver;

use crate::config::FeedbackConfig;
use crate::runtime::SnapshotBump;

pub type FeedbackEventCache = Arc<Mutex<Vec<serde_json::Value>>>;

pub struct FeedbackObserver {
    config: FeedbackConfig,
    events: FeedbackEventCache,
    rev: Arc<AtomicU64>,
    snapshot_bump: Option<SnapshotBump>,
}

impl FeedbackObserver {
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
}

impl KernelEventObserver for FeedbackObserver {
    fn on_kernel_event(&self, event: &KernelEvent) {
        if !self.config.accepts_event(event.kind, &event.tags) {
            return;
        }
        let projected = project_event(event);
        if let Ok(mut events) = self.events.lock() {
            let id = event.id.as_str();
            if events
                .iter()
                .any(|value| value.get("id").and_then(|v| v.as_str()) == Some(id))
            {
                return;
            }
            events.push(projected);
            if events.len() > self.config.max_events {
                let overflow = events.len() - self.config.max_events;
                events.drain(0..overflow);
            }
            drop(events);
            if let Some(bump) = &self.snapshot_bump {
                bump();
            } else {
                self.rev.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn project_event(event: &KernelEvent) -> serde_json::Value {
    serde_json::json!({
        "id": event.id,
        "pubkey": event.author,
        "created_at": event.created_at,
        "kind": event.kind,
        "tags": event.tags,
        "content": event.content,
        "sig": "",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, kind: u32, coordinate: &str) -> KernelEvent {
        KernelEvent {
            id: id.to_string(),
            author: "authorhex".to_string(),
            kind,
            created_at: 1_700_000_000,
            tags: vec![vec!["a".to_string(), coordinate.to_string()]],
            content: "feedback body".to_string(),
        }
    }

    #[test]
    fn observer_caches_project_event_in_signed_event_shape() {
        let config = FeedbackConfig::new("31933:abc:app");
        let events = Arc::new(Mutex::new(Vec::new()));
        let rev = Arc::new(AtomicU64::new(0));
        let observer = FeedbackObserver::new(config.clone(), events.clone(), rev.clone());

        observer.on_kernel_event(&event("id1", 1, &config.project_coordinate));

        let guard = events.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0]["id"], "id1");
        assert_eq!(guard[0]["pubkey"], "authorhex");
        assert_eq!(guard[0]["sig"], "");
        assert_eq!(rev.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn observer_ignores_unanchored_or_unrelated_kind() {
        let config = FeedbackConfig::new("31933:abc:app");
        let events = Arc::new(Mutex::new(Vec::new()));
        let rev = Arc::new(AtomicU64::new(0));
        let observer = FeedbackObserver::new(config, events.clone(), rev.clone());

        observer.on_kernel_event(&event("wrong-kind", 9802, "31933:abc:app"));
        observer.on_kernel_event(&event("wrong-project", 1, "31933:abc:other"));

        assert!(events.lock().unwrap().is_empty());
        assert_eq!(rev.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn observer_dedupes_by_event_id() {
        let config = FeedbackConfig::new("31933:abc:app");
        let events = Arc::new(Mutex::new(Vec::new()));
        let rev = Arc::new(AtomicU64::new(0));
        let observer = FeedbackObserver::new(config.clone(), events.clone(), rev.clone());

        observer.on_kernel_event(&event("dup", 513, &config.project_coordinate));
        observer.on_kernel_event(&event("dup", 513, &config.project_coordinate));

        assert_eq!(events.lock().unwrap().len(), 1);
        assert_eq!(rev.load(Ordering::Relaxed), 1);
    }
}
