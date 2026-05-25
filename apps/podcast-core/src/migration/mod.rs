//! One-shot migration from the legacy Swift Podcastr app's persistence into
//! the canonical `podcast-core` domain types.
//!
//! Legacy storage shape (per `06-cross-cutting.md` §1 and the actual Swift
//! source under `App/Sources/State/`):
//!
//! - **Metadata JSON** at App Group
//!   `<group>/Library/Application Support/podcastr-state.v1.json` — encodes
//!   `AppState` *with `episodes = []`* (see `Persistence.metadataState(from:)`
//!   in `Persistence.swift`). The actual file written under
//!   `podcastr-state.v1.json` is the metadata blob; podcasts + subscriptions
//!   live here.
//! - **Episode SQLite sidecar** at `podcastr-state.v1.episodes.sqlite` — each
//!   `Episode` is stored as a JSON `payload` blob keyed by row. Episode
//!   playback positions, `played` flags, download/transcript state live here.
//! - **Audit log files** at `Application Support/podcastr/audit/<UUID>.json`
//!   — per-episode append-only event lists.
//!
//! The iOS `pcst.legacy_io.capability` reads bytes from each of these
//! locations and hands them to Rust; *this* module decides how to merge them
//! into the new store (D7 — capabilities never decide; Rust does).
//!
//! Per D6, every parse error is data, not a panic — failures surface as
//! `MigrationError` so the caller can attach them to the snapshot's
//! `toast: Option<String>` and leave the `pcst.migration.v1.done` sentinel
//! unset for the next launch to retry.

pub mod from_episode_db;
pub mod from_state_json;

pub use from_episode_db::{from_episode_db, EpisodeDbResult};
pub use from_state_json::{from_state_json, StateJsonResult};

use crate::types::{Episode, EpisodeId, Podcast, PodcastSubscription};

/// Sentinel key (UserDefaults, NOT Keychain — this is not a secret) that the
/// kernel sets after a successful migration. A second launch sees this set
/// and the migration is a no-op.
pub const MIGRATION_DONE_KEY: &str = "pcst.migration.v1.done";

/// Aggregated, decided output of the migration. Built from the `StateJsonResult`
/// (podcasts + subscriptions, from the metadata JSON) plus the
/// `EpisodeDbResult` (episode positions + played flags, from the SQLite
/// sidecar). Either half can be empty if its source is missing or unreadable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MigrationResult {
    /// Podcasts (RSS + synthetic) recovered from the legacy state file.
    pub podcasts: Vec<Podcast>,
    /// User follow rows recovered from the legacy state file.
    pub subscriptions: Vec<PodcastSubscription>,
    /// Episodes recovered from the legacy SQLite sidecar (currently
    /// always empty — sidecar parsing is deferred; see `from_episode_db`).
    pub episodes: Vec<Episode>,
    /// `(episode_id, position_secs)` pairs the kernel should fold into
    /// the new episode store. Empty until `from_episode_db` is implemented.
    pub episode_positions: Vec<(EpisodeId, f64)>,
    /// Episode IDs the user had marked played. Empty until `from_episode_db`
    /// is implemented.
    pub episodes_played: Vec<EpisodeId>,
}

/// Errors a migration step surfaces back to the caller. Per D6, these are
/// values — they never panic across the FFI. The shell surfaces them as a
/// `toast: Option<String>` in the snapshot.
///
/// Hand-rolled instead of using `thiserror` to keep the `podcast-core`
/// dependency surface minimal — this crate is the pure-domain layer and
/// every transitive dep ships in every platform's binary.
#[derive(Debug)]
pub enum MigrationError {
    /// The legacy `podcastr-state.v1.json` blob was unreadable as `AppState`.
    MalformedStateJson(serde_json::Error),
    /// SQLite episode sidecar parsing is deferred to a future milestone.
    EpisodeDbUnsupported(&'static str),
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedStateJson(e) => write!(f, "legacy state JSON is malformed: {e}"),
            Self::EpisodeDbUnsupported(reason) => {
                write!(f, "legacy episode sidecar not yet supported: {reason}")
            }
        }
    }
}

impl std::error::Error for MigrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MalformedStateJson(e) => Some(e),
            Self::EpisodeDbUnsupported(_) => None,
        }
    }
}

impl From<serde_json::Error> for MigrationError {
    fn from(err: serde_json::Error) -> Self {
        Self::MalformedStateJson(err)
    }
}

impl MigrationResult {
    /// Compose a full result from the two sources. Either may be `None`
    /// (e.g. the capability couldn't read the file) — the partial result is
    /// still useful and we let the kernel decide whether to set the sentinel.
    pub fn combine(
        state: Option<StateJsonResult>,
        episode_db: Option<EpisodeDbResult>,
    ) -> Self {
        let mut out = MigrationResult::default();
        if let Some(state) = state {
            out.podcasts = state.podcasts;
            out.subscriptions = state.subscriptions;
        }
        if let Some(db) = episode_db {
            out.episodes = db.episodes;
            out.episode_positions = db.episode_positions;
            out.episodes_played = db.episodes_played;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_with_no_sources_is_empty() {
        let r = MigrationResult::combine(None, None);
        assert!(r.podcasts.is_empty());
        assert!(r.subscriptions.is_empty());
        assert!(r.episode_positions.is_empty());
        assert!(r.episodes_played.is_empty());
    }

    #[test]
    fn migration_done_key_uses_pcst_namespace() {
        // The plan doc historically used `podcastr.migration.v1.done`; the
        // shipping key was renamed to the canonical `pcst.` namespace so
        // every NMP-side key shares a prefix.
        assert!(MIGRATION_DONE_KEY.starts_with("pcst."));
        assert!(MIGRATION_DONE_KEY.ends_with(".done"));
    }
}
