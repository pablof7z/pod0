//! NMP-owned project feedback.
//!
//! This crate owns the reusable Nostr feedback behavior that app shells should
//! not duplicate: project-scoped kind:1/kind:513 interest construction, explicit
//! feedback-relay publish dispatch through NMP, event observation, bounded event
//! caching, and resolved thread projection.

#[cfg(feature = "nmp")]
mod command;
mod config;
#[cfg(feature = "nmp")]
mod observer;
mod projection;
#[cfg(feature = "nmp")]
mod runtime;

#[cfg(feature = "nmp")]
pub use command::{fetch_feedback, publish_feedback, FeedbackCommandOutcome};
pub use config::{
    FeedbackConfig, DEFAULT_FEEDBACK_RELAY, KIND_FEEDBACK_NOTE, KIND_FEEDBACK_THREAD_METADATA,
};
#[cfg(feature = "nmp")]
pub use observer::{FeedbackEventCache, FeedbackObserver};
pub use projection::{reduce_feedback_threads, FeedbackReplyDto, FeedbackThreadDto};
#[cfg(feature = "nmp")]
pub use runtime::{FeedbackRuntime, SnapshotBump};
