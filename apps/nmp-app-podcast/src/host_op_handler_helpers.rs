//! Free helper functions formerly inlined at the bottom of
//! `host_op_handler.rs`.
//!
//! Extracted to keep `host_op_handler.rs` under the 500-line hard limit.

use podcast_core::Episode;

/// Merge a freshly-parsed episode list against the prior in-store list,
/// preserving per-episode state that lives only in the store and would
/// otherwise be clobbered when the RSS re-parse replaces the episode record.
///
/// Carried forward for episodes matched by [`podcast_core::EpisodeId`]:
///
/// * `position_secs` — listening progress.
/// * `chapters` — but **only** AI-generated chapters, and only when the
///   freshly-parsed episode supplies none. AI chapters
///   ([`crate::ai_chapters`]) are synthesized into the store and never appear
///   in the RSS feed, so a refresh that re-parses the feed would drop them and
///   the UI would flash empty until the next snapshot rebuild
///   (`m4-chapters-rust-persistence`). Publisher chapters always win (D7): when
///   the fresh episode carries its own chapters we keep those, so a feed that
///   later ships real Podcasting 2.0 chapters cleanly supersedes the AI ones.
pub(crate) fn merge_episodes(fresh: Vec<Episode>, existing: Vec<Episode>) -> Vec<Episode> {
    fresh
        .into_iter()
        .map(|mut ep| {
            if let Some(prev) = existing.iter().find(|e| e.id == ep.id) {
                ep.position_secs = prev.position_secs;
                carry_forward_ai_chapters(&mut ep, prev);
            }
            ep
        })
        .collect()
}

/// Episode ids (string form) whose triage-relevant metadata changed between the
/// prior in-store episode and the freshly-parsed one.
///
/// A feed refresh that revises an episode's `published_at`, `title`, or
/// `description` should invalidate that episode's cached LLM triage score — the
/// score was computed from the old metadata and may no longer reflect the
/// episode. Returns the ids of episodes present in BOTH lists (matched by
/// [`podcast_core::EpisodeId`]) whose `pub_date`, `title`, or `description`
/// differs. Brand-new episodes have no cache entry to invalidate, and dropped
/// episodes are handled by the cache's own staleness/eviction, so neither is
/// reported here.
///
/// Pure (no locks, no IO) so it is unit-testable and leaves `merge_episodes`'s
/// signature — and its existing tests — untouched. The caller invalidates the
/// triage cache for the returned ids after releasing the store lock.
pub(crate) fn changed_metadata_ids(fresh: &[Episode], existing: &[Episode]) -> Vec<String> {
    fresh
        .iter()
        .filter_map(|ep| {
            existing.iter().find(|e| e.id == ep.id).and_then(|prev| {
                let changed = prev.pub_date != ep.pub_date
                    || prev.title != ep.title
                    || prev.description != ep.description;
                changed.then(|| ep.id.0.to_string())
            })
        })
        .collect()
}

/// True when an episode carries usable chapters — i.e. `Some(non-empty)`.
/// Mirrors the "loaded" notion in
/// [`crate::store::PodcastStore::episode_chapters_state`] so the merge gate and
/// the AI-compile gate agree on what "has chapters" means.
fn has_chapters(ep: &Episode) -> bool {
    ep.chapters.as_ref().map(|c| !c.is_empty()).unwrap_or(false)
}

/// Copy AI-generated chapters from `prev` onto `ep` when `ep` brought none of
/// its own. Only AI chapters are carried — prior publisher chapters that
/// vanished from the re-parsed feed are intentionally allowed to drop.
fn carry_forward_ai_chapters(ep: &mut Episode, prev: &Episode) {
    if has_chapters(ep) {
        return; // Fresh chapters (publisher or otherwise) win — D7.
    }
    if let Some(prev_chapters) = prev.chapters.as_ref() {
        let ai: Vec<_> = prev_chapters.iter().filter(|c| c.is_ai_generated).cloned().collect();
        if !ai.is_empty() {
            ep.chapters = Some(ai);
        }
    }
}

#[cfg(test)]
#[path = "host_op_handler_helpers_tests.rs"]
mod tests;
