//! Parse Nostr events (raw `Vec<Vec<String>>` tags + header fields) into
//! the [`crate::types::NIP74Show`] / [`crate::types::NIP74Episode`] views.
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
mod tests {
    use super::*;

    fn tags() -> Vec<Vec<String>> {
        vec![
            vec!["d".into(), "show-1".into()],
            vec!["title".into(), "My Show".into()],
            vec!["t".into(), "Tech".into()],
            vec!["t".into(), "News".into()],
            vec!["image".into()], // malformed — value missing
            vec!["empty".into(), String::new()],
        ]
    }

    #[test]
    fn first_tag_value_returns_value_when_present() {
        let t = tags();
        assert_eq!(first_tag_value(&t, "title"), Some("My Show"));
        assert_eq!(first_tag_value(&t, "d"), Some("show-1"));
    }

    #[test]
    fn first_tag_value_returns_none_when_missing_or_empty() {
        let t = tags();
        assert_eq!(first_tag_value(&t, "summary"), None);
        // tag present but value missing
        assert_eq!(first_tag_value(&t, "image"), None);
        // tag present, value is empty string
        assert_eq!(first_tag_value(&t, "empty"), None);
    }

    #[test]
    fn all_tag_values_collects_repeats_in_order() {
        let t = tags();
        assert_eq!(all_tag_values(&t, "t"), vec!["Tech", "News"]);
        assert!(all_tag_values(&t, "missing").is_empty());
    }

    #[test]
    fn first_tag_returns_full_slice_for_imeta_style() {
        let t = vec![vec![
            "imeta".into(),
            "url https://a.example/x.mp3".into(),
            "m audio/mp4".into(),
        ]];
        let imeta = first_tag(&t, "imeta").expect("present");
        assert_eq!(imeta.len(), 3);
        assert_eq!(imeta[1], "url https://a.example/x.mp3");
    }
}
