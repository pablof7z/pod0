//! Queue-projection helper for [`super::snapshot::build_snapshot_payload`].
//!
//! Extracted into a sibling module so [`super::snapshot`] stays under the
//! 500-line ceiling once additional projections land. The single helper
//! cross-references the queued episode-id list against the freshly-built
//! library projection so the iOS list can render artwork + podcast title
//! per row without a second pull.

use super::projections::{EpisodeSummary, PodcastSummary};

/// Cross-reference queued episode ids against the freshly-built library
/// projection so each queue row carries the metadata the iOS list needs
/// (title, artwork, podcast title). Ids whose episode is no longer in the
/// library (e.g. the user unsubscribed after queuing) are silently dropped —
/// the queue itself still holds them, but the UI projection won't render
/// orphaned rows.
pub(super) fn resolve_queue_rows(
    ids: &[String],
    library: &[PodcastSummary],
) -> Vec<EpisodeSummary> {
    ids.iter()
        .filter_map(|id| {
            library
                .iter()
                .flat_map(|p| p.episodes.iter())
                .find(|ep| ep.id == *id)
                .cloned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_unknown_ids() {
        let library = vec![PodcastSummary {
            id: "p1".into(),
            title: "Show".into(),
            episodes: vec![EpisodeSummary {
                id: "ep-1".into(),
                title: "Pilot".into(),
                ..EpisodeSummary::default()
            }],
            ..PodcastSummary::default()
        }];
        let rows = resolve_queue_rows(
            &["ep-1".to_owned(), "ep-missing".to_owned()],
            &library,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "ep-1");
    }

    #[test]
    fn preserves_order() {
        let library = vec![PodcastSummary {
            id: "p1".into(),
            title: "Show".into(),
            episodes: vec![
                EpisodeSummary {
                    id: "ep-a".into(),
                    title: "A".into(),
                    ..EpisodeSummary::default()
                },
                EpisodeSummary {
                    id: "ep-b".into(),
                    title: "B".into(),
                    ..EpisodeSummary::default()
                },
            ],
            ..PodcastSummary::default()
        }];
        let rows = resolve_queue_rows(
            &["ep-b".to_owned(), "ep-a".to_owned()],
            &library,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "ep-b");
        assert_eq!(rows[1].id, "ep-a");
    }

    #[test]
    fn empty_ids_yields_empty_rows() {
        let library: Vec<PodcastSummary> = vec![];
        let rows = resolve_queue_rows(&[], &library);
        assert!(rows.is_empty());
    }

    #[test]
    fn podcast_update_queue_round_trips_via_serde() {
        // Confirm the new `queue` field on PodcastUpdate survives a JSON
        // round-trip and that an empty queue is omitted from the wire
        // payload (D5 byte-identity).
        use super::super::snapshot::PodcastUpdate;
        let snap = PodcastUpdate {
            queue: vec![EpisodeSummary {
                id: "ep-1".into(),
                title: "Pilot".into(),
                ..EpisodeSummary::default()
            }],
            ..PodcastUpdate::default()
        };
        let json = serde_json::to_string(&snap).expect("encode");
        assert!(json.contains("\"queue\""));
        let decoded: PodcastUpdate = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded.queue.len(), 1);
        assert_eq!(decoded.queue[0].id, "ep-1");

        // Empty queue must not appear on the wire.
        let default_json =
            serde_json::to_string(&PodcastUpdate::default()).expect("encode");
        assert!(!default_json.contains("\"queue\""));
    }
}
