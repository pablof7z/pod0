//! Parse Nostr events (raw `Vec<Vec<String>>` tags + header fields) into
//! the [`crate::types::NipF4DiscoveryShow`] / [`crate::types::NipF4DiscoveryEpisode`] views.
//!
//! Mapping into the `podcast_core` domain types lives next to each parser
//! so the cross-crate boundary is co-located with the tag layout it
//! consumes (per AGENTS.md "TEA organization" — split by cohesive
//! ownership, not technical role).

mod episode;
mod episode_map;
mod imeta;
mod show;

pub use episode::parse_episode_event;
pub use episode_map::episode_to_episode;
pub use show::{parse_show_event, show_to_podcast};

/// Find the first tag named `name` and return its second component.
/// Returns `None` when the tag isn't present or its value is empty.
///
/// Mirrors the Swift `tags.first(where: { $0.first == name })?[safe: 1]`
/// idiom — keeps callers tag-agnostic.
pub(crate) fn first_tag_value<'a>(tags: &'a [Vec<String>], name: &str) -> Option<&'a str> {
    tags.iter()
        .find(|tag| tag.first().map(String::as_str) == Some(name))
        .and_then(|tag| tag.get(1).map(String::as_str))
        .filter(|s| !s.is_empty())
}

/// Find the first tag named `name` and return its full payload (positions
/// 1..). Used by `imeta` and `chapters`/`transcript` (which carry a MIME
/// type at position 2).
pub(crate) fn first_tag<'a>(tags: &'a [Vec<String>], name: &str) -> Option<&'a [String]> {
    tags.iter()
        .find(|tag| tag.first().map(String::as_str) == Some(name))
        .map(|tag| tag.as_slice())
}

/// All values of repeated single-value tags (e.g. `["t", "Tech"]`).
pub(crate) fn all_tag_values(tags: &[Vec<String>], name: &str) -> Vec<String> {
    tags.iter()
        .filter(|tag| tag.first().map(String::as_str) == Some(name))
        .filter_map(|tag| tag.get(1).cloned())
        .filter(|v| !v.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
