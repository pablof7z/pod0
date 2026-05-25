//! Parse the legacy episode SQLite sidecar.
//!
//! **Deferred.** The legacy `EpisodeSQLiteStore` (see
//! `App/Sources/State/EpisodeSQLiteStore.swift`) stores each episode as a
//! JSON `payload` blob in the `episodes` table, with `sort_order` driving
//! display order. Parsing it from Rust requires a SQLite dependency
//! (`rusqlite` or similar) and is not on the critical path for M2.D — the
//! podcast/subscription metadata from `from_state_json` is sufficient to
//! light up the Library tab.
//!
//! When this lands, the contract is:
//!
//! ```text
//! fn from_episode_db(db_bytes: &[u8]) -> Result<EpisodeDbResult, MigrationError>
//!     1. Open the SQLite DB from the byte buffer (or — preferred — from a
//!        temp-file path the capability writes to, since rusqlite-from-bytes
//!        is awkward).
//!     2. SELECT payload FROM episodes ORDER BY sort_order ASC.
//!     3. JSON-decode each payload via a `LegacyEpisode` shadow struct.
//!     4. Project to `Episode` + position-and-played tuples.
//! ```
//!
//! Per the migration plan, on failure the toast surfaces and the sentinel
//! stays unset (D6). Episodes without positions/played-flags simply
//! recover empty in the meantime — the user keeps their subscriptions and
//! the first feed refresh re-populates the episode rows from RSS.

use crate::types::{Episode, EpisodeId};

use super::MigrationError;

/// Result of parsing the episode sidecar. Always empty until the SQLite
/// reader is implemented.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EpisodeDbResult {
    pub episodes: Vec<Episode>,
    pub episode_positions: Vec<(EpisodeId, f64)>,
    pub episodes_played: Vec<EpisodeId>,
}

/// Stub. Returns `EpisodeDbUnsupported`; callers degrade gracefully and
/// surface the toast. The episode rows will repopulate from RSS on the
/// first subscription refresh after migration, so the user-visible cost
/// is only the loss of playback positions for episodes the legacy build
/// had partially listened to.
pub fn from_episode_db(_db_bytes: &[u8]) -> Result<EpisodeDbResult, MigrationError> {
    Err(MigrationError::EpisodeDbUnsupported(
        "rusqlite reader not yet wired in this milestone — episodes \
         repopulate from RSS on first refresh",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_episode_db_returns_unsupported_until_implemented() {
        // The capability should still happily call us; the shell handles
        // the error envelope.
        let err = from_episode_db(b"").expect_err("stub must error");
        assert!(matches!(err, MigrationError::EpisodeDbUnsupported(_)));
        // Display impl must be human-readable — toast surfaces this verbatim.
        assert!(format!("{err}").contains("not yet supported"));
    }

    #[test]
    fn empty_result_default_is_truly_empty() {
        let r = EpisodeDbResult::default();
        assert!(r.episodes.is_empty());
        assert!(r.episode_positions.is_empty());
        assert!(r.episodes_played.is_empty());
    }
}
