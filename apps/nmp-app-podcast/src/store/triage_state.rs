//! M4 capability-report side-maps: AI Inbox triage, RAG metadata-indexed
//! coverage, and transient transcript-ingestion status.
//!
//! These three concerns were previously preserved Swift-only across kernel
//! projection passes (the deleted "preserved-state block" in
//! `AppStateStore+KernelProjection.swift`). Per D7 the iOS services that
//! compute them now *report* the result to the kernel; Rust stores it here and
//! projects it onto `EpisodeSummary` so the data rides the reactive push frame
//! and survives feed refreshes.
//!
//! Lives in its own file (rather than expanding `mod.rs`) to keep the store
//! focused; every mutator calls `self.persist()` so the state survives an app
//! restart — matching the `ad_segments` module's discipline.

use super::PodcastStore;

impl PodcastStore {
    // ── Triage ────────────────────────────────────────────────────────────

    /// Return the stored triage tuple `(decision, is_hero, rationale)` for
    /// `episode_id_str` (UUID hyphenated string form), or `None` when the
    /// episode is untriaged.
    pub fn triage_for(&self, episode_id_str: &str) -> Option<&(String, bool, Option<String>)> {
        self.episode_triage.get(episode_id_str)
    }

    /// Record the AI Inbox triage decision for an episode. `decision` is the
    /// raw `TriageDecision` rawValue (`"inbox"` / `"archived"`); the sentinel
    /// `"none"` clears any prior decision (user-rescue / re-triage path).
    ///
    /// Returns `true` when the stored state actually changed (so the caller
    /// can decide whether to bump `rev`). Idempotent — a no-op write neither
    /// mutates the map nor flushes to disk.
    pub fn set_episode_triage(
        &mut self,
        episode_id_str: impl Into<String>,
        decision: &str,
        is_hero: bool,
        rationale: Option<String>,
    ) -> bool {
        let key = episode_id_str.into();
        // The sentinel "none" clears the entry entirely so the episode reads
        // back as untriaged (matches `clearTriageDecision` on the iOS side).
        if decision == "none" {
            let changed = self.episode_triage.remove(&key).is_some();
            if changed {
                self.persist();
            }
            return changed;
        }
        // Archived episodes never carry a rationale (the user isn't meant to
        // audit them); mirror the iOS `applyTriageDecisions` invariant so the
        // two stores can't drift.
        let normalized_rationale = if decision == "inbox" { rationale } else { None };
        let next = (decision.to_owned(), is_hero, normalized_rationale);
        if self.episode_triage.get(&key) == Some(&next) {
            return false;
        }
        self.episode_triage.insert(key, next);
        self.persist();
        true
    }

    // ── Metadata-indexed coverage ───────────────────────────────────────────

    /// `true` when the episode's metadata (or transcript) chunk has been
    /// embedded into the RAG index.
    pub fn is_metadata_indexed(&self, episode_id_str: &str) -> bool {
        self.metadata_indexed_episodes.contains(episode_id_str)
    }

    /// Mark a batch of episode ids as covered by the RAG metadata index.
    /// Returns `true` when at least one id was newly inserted. Flushes once
    /// for the whole batch so a large backfill pays a single `persist()`.
    pub fn mark_episodes_metadata_indexed<I, S>(&mut self, episode_ids: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut changed = false;
        for id in episode_ids {
            if self.metadata_indexed_episodes.insert(id.into()) {
                changed = true;
            }
        }
        if changed {
            self.persist();
        }
        changed
    }

    // ── Transcript status override ──────────────────────────────────────────

    /// Return the stored transient transcript status tuple
    /// `(status, message)` for `episode_id_str`, or `None` when no override is
    /// recorded (idle / cleared / `.ready` derived from `transcript`).
    pub fn transcript_status_for(&self, episode_id_str: &str) -> Option<&(String, Option<String>)> {
        self.transcript_status_overrides.get(episode_id_str)
    }

    /// Record the transient transcript-ingestion status for an episode.
    /// `status` is one of `"queued"` | `"fetching_publisher"` |
    /// `"transcribing"` | `"failed"`; the sentinel `"none"` clears the
    /// override (used when the pipeline reaches `.ready`, since `.ready` is
    /// derived from the stored `transcript`, or when it returns to idle).
    /// `message` carries the user-facing error text for `"failed"`.
    ///
    /// Returns `true` when the stored state changed. Idempotent.
    pub fn set_transcript_status(
        &mut self,
        episode_id_str: impl Into<String>,
        status: &str,
        message: Option<String>,
    ) -> bool {
        let key = episode_id_str.into();
        if status.is_empty() || status == "none" {
            let changed = self.transcript_status_overrides.remove(&key).is_some();
            if changed {
                self.persist();
            }
            return changed;
        }
        // Only "failed" carries a message; drop it for other statuses so the
        // wire shape stays clean and the two states can't drift.
        let normalized_message = if status == "failed" { message } else { None };
        let next = (status.to_owned(), normalized_message);
        if self.transcript_status_overrides.get(&key) == Some(&next) {
            return false;
        }
        self.transcript_status_overrides.insert(key, next);
        self.persist();
        true
    }
}

#[cfg(test)]
#[path = "triage_state_tests.rs"]
mod tests;
