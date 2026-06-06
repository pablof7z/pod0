//! Builds the [`AgentContextSnapshot`] inventory projection.
//!
//! This is the kernel-side home of the policy the iOS `AgentPrompt` builder
//! used to compute itself: which subscribed shows to list, which in-progress
//! episodes to surface, and which recent-unplayed episodes fall inside the
//! recency window. Moving it here keeps "what the agent knows about the
//! user's library" a single kernel-owned decision instead of duplicated
//! filter predicates on the Swift side.
//!
//! Built from the already-assembled `library: &[PodcastSummary]` (see
//! [`super::snapshot::build_podcast_update`]) so it reuses the resolved
//! `playback_position_secs` / `played` / `triage_decision` / `published_at`
//! fields without taking a second store lock. The kernel `library`
//! projection is exactly the user's followed set (the iOS
//! `applyKernelState` derives one subscription row per library entry), so
//! listing every library row reproduces the Swift "followed podcasts only"
//! filter precisely.

use super::projections::{AgentContextEpisode, AgentContextSnapshot, PodcastSummary};

/// Policy caps — ported verbatim from the iOS `AgentPrompt.Cap` enum so this
/// is a behavior-preserving relocation, not a behavior change.
pub(crate) mod cap {
    /// Max subscribed shows listed (alphabetical, then capped).
    pub const SUBSCRIPTIONS: usize = 30;
    /// Max in-progress episodes listed (newest-first, then capped).
    pub const IN_PROGRESS: usize = 5;
    /// Max recent-unplayed episodes listed (newest-first, then capped).
    pub const RECENT_UNPLAYED: usize = 10;
    /// Recency window (days) applied to the recent-unplayed list.
    pub const RECENT_WINDOW_DAYS: u32 = 7;
}

/// `"archived"` triage decision — episodes the AI Inbox silently hid. The
/// iOS prompt filtered these out via `Episode.isTriageArchived`; we mirror
/// the same string the projection layer stamps on `triage_decision`.
const TRIAGE_ARCHIVED: &str = "archived";

/// Build the agent-context inventory snapshot from the assembled library and
/// the current wall-clock instant (Unix seconds).
///
/// `now_unix` is injected rather than read from `SystemTime` inside so tests
/// can pin a deterministic recency cutoff.
pub fn build_agent_context(library: &[PodcastSummary], now_unix: i64) -> AgentContextSnapshot {
    // ── Subscriptions: the snapshot builder passes only followed shows here. Sort
    // case-insensitively by title (matches Swift's
    // `localizedCaseInsensitiveCompare`), then cap. ──────────────────────
    let mut titles: Vec<&str> = library.iter().map(|p| p.title.as_str()).collect();
    titles.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    let subscriptions_total = titles.len();
    let subscriptions: Vec<String> = titles
        .into_iter()
        .take(cap::SUBSCRIPTIONS)
        .map(str::to_owned)
        .collect();

    // ── Episodes: flatten across ALL shows, then sort the flat list by
    // published_at newest-first (matches Swift's global `state.episodes`
    // sort), then filter + cap each list independently. ─────────────────
    struct Row<'a> {
        title: &'a str,
        show_title: &'a str,
        published_at: i64,
        played: bool,
        archived: bool,
        position: f64,
    }
    let mut rows: Vec<Row<'_>> = library
        .iter()
        .flat_map(|podcast| {
            podcast.episodes.iter().map(move |ep| Row {
                title: ep.title.as_str(),
                show_title: podcast.title.as_str(),
                published_at: ep.published_at.unwrap_or(0),
                played: ep.played,
                archived: ep.triage_decision.as_deref() == Some(TRIAGE_ARCHIVED),
                // `playback_position_secs` is `None` once a show is fresh
                // (Rust projects `None` for a zero position), so treat the
                // absent case as 0.0 — matches Swift's `playbackPosition`.
                position: ep.playback_position_secs.unwrap_or(0.0),
            })
        })
        .collect();
    // Newest-first by publish date. Stable so equal dates keep library order.
    rows.sort_by(|a, b| b.published_at.cmp(&a.published_at));

    let to_episode = |r: &Row<'_>| AgentContextEpisode {
        title: r.title.to_owned(),
        show_title: r.show_title.to_owned(),
    };

    // In-progress: not played, not archived, started (position > 0).
    let in_progress: Vec<AgentContextEpisode> = rows
        .iter()
        .filter(|r| !r.played && !r.archived && r.position > 0.0)
        .take(cap::IN_PROGRESS)
        .map(to_episode)
        .collect();

    // Recent unplayed: not played, not archived, not started, inside window.
    let cutoff = now_unix - i64::from(cap::RECENT_WINDOW_DAYS) * 86_400;
    let recent_unplayed: Vec<AgentContextEpisode> = rows
        .iter()
        .filter(|r| !r.played && !r.archived && r.position == 0.0 && r.published_at >= cutoff)
        .take(cap::RECENT_UNPLAYED)
        .map(to_episode)
        .collect();

    AgentContextSnapshot {
        subscriptions,
        subscriptions_total,
        in_progress,
        recent_unplayed,
        recent_window_days: cap::RECENT_WINDOW_DAYS,
    }
}

#[cfg(test)]
#[path = "agent_context_tests.rs"]
mod tests;
