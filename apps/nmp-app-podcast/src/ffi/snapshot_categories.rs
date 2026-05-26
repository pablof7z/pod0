//! Category-aggregate helper for [`super::snapshot::build_snapshot_payload`].
//!
//! Extracted into a sibling module so [`super::snapshot`] stays under the
//! 500-line ceiling. Rolls the per-episode `ai_categories` labels up into
//! one [`CategoryBrowseItem`] per category, ordered by the most recent
//! contribution (so the category whose newest episode is freshest renders
//! first in the iOS grid).

use super::projections::{CategoryBrowseItem, PodcastSummary};

/// Roll the per-episode `ai_categories` labels up into one
/// [`CategoryBrowseItem`] per category, ordered by the most recent
/// contribution (so the category whose newest episode is freshest
/// renders first in the iOS grid).
///
/// `top_episode_ids` holds the three most recently-published episode ids
/// for the category, newest-first by `published_at`. Tied timestamps
/// fall back to library iteration order, which is stable across ticks
/// for the same store contents.
pub(super) fn build_category_aggregate(library: &[PodcastSummary]) -> Vec<CategoryBrowseItem> {
    use std::collections::{BTreeSet, HashMap};

    struct Bucket {
        episode_ids_by_recency: Vec<(i64, String)>,
        podcast_ids: BTreeSet<String>,
        latest: i64,
    }
    let mut buckets: HashMap<String, Bucket> = HashMap::new();

    for podcast in library {
        for ep in &podcast.episodes {
            let published = ep.published_at.unwrap_or(0);
            for cat in &ep.ai_categories {
                let entry = buckets.entry(cat.clone()).or_insert_with(|| Bucket {
                    episode_ids_by_recency: Vec::new(),
                    podcast_ids: BTreeSet::new(),
                    latest: i64::MIN,
                });
                entry.episode_ids_by_recency.push((published, ep.id.clone()));
                entry.podcast_ids.insert(podcast.id.clone());
                if published > entry.latest {
                    entry.latest = published;
                }
            }
        }
    }

    let mut items: Vec<(i64, CategoryBrowseItem)> = buckets
        .into_iter()
        .map(|(category, mut bucket)| {
            // Newest-first; tie-break by insertion order via stable sort.
            bucket
                .episode_ids_by_recency
                .sort_by(|a, b| b.0.cmp(&a.0));
            let top_episode_ids = bucket
                .episode_ids_by_recency
                .iter()
                .take(3)
                .map(|(_, id)| id.clone())
                .collect();
            let item = CategoryBrowseItem {
                category,
                episode_count: bucket.episode_ids_by_recency.len(),
                podcast_count: bucket.podcast_ids.len(),
                top_episode_ids,
                ad_segments: vec![],
            };
            (bucket.latest, item)
        })
        .collect();

    // Category-level order: newest contributing episode first; ties by
    // category name so the snapshot is deterministic for the same store.
    items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.category.cmp(&b.1.category)));

    items.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::projections::EpisodeSummary;

    fn make_podcast(id: &str, episodes: Vec<EpisodeSummary>) -> PodcastSummary {
        PodcastSummary {
            id: id.into(),
            title: id.into(),
            episodes,
            ..PodcastSummary::default()
        }
    }

    fn make_episode(id: &str, categories: Vec<&str>, published_at: Option<i64>) -> EpisodeSummary {
        EpisodeSummary {
            id: id.into(),
            title: id.into(),
            ai_categories: categories.into_iter().map(|s| s.to_string()).collect(),
            published_at,
            ..EpisodeSummary::default()
        }
    }

    #[test]
    fn empty_library_yields_empty_categories() {
        assert!(build_category_aggregate(&[]).is_empty());
    }

    #[test]
    fn single_category_aggregates_correctly() {
        let library = vec![make_podcast(
            "p1",
            vec![
                make_episode("ep1", vec!["Tech"], Some(100)),
                make_episode("ep2", vec!["Tech"], Some(200)),
            ],
        )];
        let result = build_category_aggregate(&library);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].category, "Tech");
        assert_eq!(result[0].episode_count, 2);
        assert_eq!(result[0].podcast_count, 1);
        assert_eq!(result[0].top_episode_ids[0], "ep2");
    }

    #[test]
    fn ordered_newest_category_first() {
        let library = vec![make_podcast(
            "p1",
            vec![
                make_episode("ep1", vec!["Old"], Some(10)),
                make_episode("ep2", vec!["New"], Some(999)),
            ],
        )];
        let result = build_category_aggregate(&library);
        assert_eq!(result[0].category, "New");
        assert_eq!(result[1].category, "Old");
    }

    #[test]
    fn top_episode_ids_capped_at_three() {
        let episodes: Vec<EpisodeSummary> = (0..5)
            .map(|i| make_episode(&format!("ep{i}"), vec!["Cat"], Some(i as i64)))
            .collect();
        let library = vec![make_podcast("p1", episodes)];
        let result = build_category_aggregate(&library);
        assert_eq!(result[0].top_episode_ids.len(), 3);
    }
}
