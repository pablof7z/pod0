use serde::{Deserialize, Serialize};

/// Agent-prompt inventory context surfaced via
/// [`super::snapshot::PodcastUpdate::agent_context`].
///
/// This is the kernel-owned *policy* output the iOS `AgentPrompt` builder
/// used to compute itself: which subscribed shows to list, which in-progress
/// episodes to surface, and which recent-unplayed episodes fall inside the
/// recency window. The kernel performs all selection, ordering, capping, and
/// show-title resolution; Swift only renders the strings (section headers,
/// title truncation, bullet joining).
///
/// Each list is already sorted + capped by the projection layer
/// ([`super::super::agent_context::build_agent_context`]) so the iOS shell
/// renders it top-to-bottom without re-sorting or re-filtering. The
/// `*_total` counts carry the pre-cap totals so the renderer can emit the
/// "…and N more" suffix without re-deriving the full set.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentContextSnapshot {
    /// Subscribed-show titles to list in the prompt's `## Subscriptions`
    /// section, already sorted + capped. Synthetic shows (no follow row)
    /// are excluded by the projection layer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<String>,
    /// Total number of followed shows *before* the cap, so the renderer can
    /// emit "…and N more" and the "(N)" header count without the full list.
    pub subscriptions_total: usize,
    /// In-progress episodes (started but not finished, not archived),
    /// newest-first, capped. Each row carries its resolved show title so the
    /// renderer needs no second lookup.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub in_progress: Vec<AgentContextEpisode>,
    /// Recent unplayed episodes inside the recency window (not started, not
    /// archived), newest-first, capped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_unplayed: Vec<AgentContextEpisode>,
    /// The recency-window width (in days) the kernel applied to
    /// `recent_unplayed`. Surfaced so the renderer can label the section
    /// ("Recent (last N days, unplayed)") without hardcoding the policy.
    pub recent_window_days: u32,
}

/// One episode row in [`AgentContextSnapshot::in_progress`] /
/// [`AgentContextSnapshot::recent_unplayed`].
///
/// Narrow on purpose: the prompt renderer only needs the title and the
/// owning show's title. The owning-show title is pre-resolved by the
/// projection layer (denormalized from the library) so Swift does not have
/// to re-join against `library`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct AgentContextEpisode {
    /// Episode title (untruncated — the renderer applies its own char cap).
    pub title: String,
    /// Owning show's title, pre-resolved by the projection layer.
    pub show_title: String,
}
