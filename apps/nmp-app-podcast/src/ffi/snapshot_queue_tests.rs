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
        &[
            crate::queue::QueuedPlaybackItem::whole_episode("ep-1"),
            crate::queue::QueuedPlaybackItem::whole_episode("ep-missing"),
        ],
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
        &[
            crate::queue::QueuedPlaybackItem::whole_episode("ep-b"),
            crate::queue::QueuedPlaybackItem::whole_episode("ep-a"),
        ],
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
    let default_json = serde_json::to_string(&PodcastUpdate::default()).expect("encode");
    assert!(!default_json.contains("\"queue\""));
}
