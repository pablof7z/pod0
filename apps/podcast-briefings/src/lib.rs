//! `podcast-briefings` — briefing composition + scheduling domain.
//!
//! Types, actions, and a pure-data scheduler for the podcast app's daily
//! briefings. This is the M9.A skeleton: the composer, stitcher,
//! player engine, and agent-tool dispatcher all land in subsequent
//! milestones (M9.B–C).
//!
//! ## Scope
//!
//! * Domain: [`Briefing`], [`BriefingSegment`], [`BriefingStatus`],
//!   [`BriefingSchedule`] — port of the legacy Swift `Briefing/*` files
//!   into Rust, narrowed to the M9.A composition/playback contract.
//! * Actions: [`RequestBriefingAction`], [`ScheduleBriefingAction`],
//!   [`CancelBriefingAction`] plus their stable `podcast.briefing.*` ids.
//! * Scheduler: [`BriefingScheduler`] — synchronous, clock-free state
//!   machine the kernel-side `ActionModule` impls will call into in M9.B.
//!
//! ## Doctrine
//!
//! * **Pure** — no async, no I/O, no kernel deps, no `Utc::now()` inside
//!   the state machine. Tests pass `now_minutes_since_midnight` +
//!   `day_of_week` explicitly so the scheduler is deterministic. (The
//!   kernel-side action module supplies the wall clock; the scheduler
//!   merely projects.)
//! * **D6 alignment** — every type is `Serialize` + `Deserialize`;
//!   the snapshot serializer in `nmp-app-podcast` re-exports the wire
//!   shapes.
//! * **D7 alignment** — composition policy (segment ordering, time slot,
//!   day filtering) lives here, never in the iOS capability layer.
//! * **300 LOC soft / 500 LOC hard** per file (matches AGENTS.md).

pub mod actions;
pub mod scheduler;
pub mod types;

pub use actions::{
    CancelBriefingAction, RequestBriefingAction, ScheduleBriefingAction, ACTION_BRIEFING_CANCEL,
    ACTION_BRIEFING_REQUEST, ACTION_BRIEFING_SCHEDULE,
};
pub use scheduler::BriefingScheduler;
pub use types::{
    Briefing, BriefingSchedule, BriefingSegment, BriefingStatus, SegmentKind,
};
