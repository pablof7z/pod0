//! Free helper functions formerly inlined at the bottom of
//! `host_op_handler.rs`.
//!
//! Extracted to keep `host_op_handler.rs` under the 500-line hard limit.

use podcast_core::Episode;

/// Merge a freshly-parsed episode list against the prior in-store list,
/// preserving `position_secs` so a refresh does not lose listening progress.
pub(crate) fn merge_episodes(fresh: Vec<Episode>, existing: Vec<Episode>) -> Vec<Episode> {
    fresh
        .into_iter()
        .map(|mut ep| {
            if let Some(prev) = existing.iter().find(|e| e.id == ep.id) {
                ep.position_secs = prev.position_secs;
            }
            ep
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use podcast_core::PodcastId;
    use url::Url;

    fn ep(title: &str, position: f64) -> Episode {
        let mut e = Episode::new(
            PodcastId::generate(),
            "https://example.com/feed.xml",
            title,
            title,
            Url::parse("https://example.com/audio.mp3").unwrap(),
            Utc::now(),
        );
        e.position_secs = position;
        e
    }

    #[test]
    fn merge_preserves_existing_position_for_matching_ids() {
        let existing = vec![ep("A", 42.0), ep("B", 100.0)];
        let mut fresh = existing.iter().map(|e| {
            let mut e2 = e.clone();
            e2.position_secs = 0.0;
            e2
        }).collect::<Vec<_>>();
        fresh.push(ep("C", 0.0));

        let merged = merge_episodes(fresh, existing);
        assert_eq!(merged[0].position_secs, 42.0);
        assert_eq!(merged[1].position_secs, 100.0);
        assert_eq!(merged[2].position_secs, 0.0);
    }

    #[test]
    fn merge_returns_empty_when_fresh_is_empty() {
        let existing = vec![ep("A", 42.0)];
        assert!(merge_episodes(vec![], existing).is_empty());
    }
}
