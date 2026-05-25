//! Helpers shared by [`crate::host_op_handler`] and any future refresh
//! pipeline that needs to fold incoming episodes from a parsed feed
//! into an existing store snapshot.

use podcast_core::Episode;

/// Merge `fresh` episodes (just parsed from a feed) with `existing`
/// episodes (already in the store), preserving the existing
/// `position_secs` for any episode that survives.
///
/// Used by `refresh_one` so resubscribing or re-fetching a feed does
/// not blow away playback progress on episodes the user has already
/// started.
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
            "guid",
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
        // Add a brand-new episode that has no prior position.
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
