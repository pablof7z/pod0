use super::*;

#[test]
fn plans_bounded_percent_encoded_directory_search() {
    let plan = plan_directory_search("  hidden brain & habits  ", 500).unwrap();
    assert!(plan.url.contains("term=hidden+brain+%26+habits"));
    assert!(plan.url.contains("limit=50"));
}

#[test]
fn parser_drops_invalid_feeds_and_prefers_large_artwork() {
    let rows = parse_directory_response(
        br#"{"results":[
          {"collectionId":1,"collectionName":"Good","feedUrl":"https://example.test/feed","artworkUrl600":"https://example.test/600.jpg","artworkUrl100":"https://example.test/100.jpg"},
          {"collectionId":2,"collectionName":"Bad","feedUrl":"not a url"}
        ]}"#,
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].artwork_url.as_deref(),
        Some("https://example.test/600.jpg")
    );
}

#[test]
fn chart_lookup_preserves_chart_order() {
    let ids = parse_top_chart_ids(br#"{"feed":{"results":[{"id":"9"},{"id":"4"}]}}"#).unwrap();
    let rows = parse_directory_response(
        br#"{"results":[
          {"collectionId":4,"collectionName":"Four","feedUrl":"https://four.test/feed"},
          {"collectionId":9,"collectionName":"Nine","feedUrl":"https://nine.test/feed"}
        ]}"#,
    )
    .unwrap();
    let ordered = order_directory_results(rows, &ids);
    assert_eq!(
        ordered
            .iter()
            .map(|row| row.collection_id)
            .collect::<Vec<_>>(),
        vec![9, 4]
    );
}
