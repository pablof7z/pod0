use pod0_application::{LibraryDocumentObservation, LibraryNetworkIntent, LibraryNetworkStep};

pub(super) fn catalog_directory_action(
    document: &LibraryDocumentObservation,
) -> Option<pod0_storage::LibraryNetworkObservationAction> {
    let feed_urls: Vec<_> = pod0_application::parse_directory_response(&document.bytes)
        .ok()?
        .into_iter()
        .take(8)
        .map(|entry| entry.feed_url)
        .collect();
    let Some(first) = feed_urls.first().cloned() else {
        return Some(pod0_storage::LibraryNetworkObservationAction::CompleteCatalog {
            candidates: Vec::new(),
        });
    };
    Some(pod0_storage::LibraryNetworkObservationAction::ContinueCatalog {
        step: LibraryNetworkStep::CatalogFeed {
            feed_urls,
            ordinal: 0,
            candidates: Vec::new(),
        },
        request: pod0_application::plan_shared_feed_request(&first)?,
    })
}

pub(super) fn catalog_feed_action(
    intent: &LibraryNetworkIntent,
    document: &LibraryDocumentObservation,
    feed_urls: &[String],
    ordinal: u16,
    mut candidates: Vec<pod0_application::CatalogEpisodeCandidate>,
    observed_at_ms: i64,
) -> Option<pod0_storage::LibraryNetworkObservationAction> {
    let LibraryNetworkIntent::CatalogEpisodeSearch {
        episode_query,
        podcast_hint,
        limit,
    } = intent else {
        return None;
    };
    if let Ok(mut found) = pod0_application::catalog_candidates_from_feed(
        &document.bytes,
        &document.response_url,
        episode_query,
        podcast_hint.as_deref(),
        observed_at_ms,
    ) {
        candidates.append(&mut found);
    }
    let next = ordinal.saturating_add(1);
    if let Some(feed_url) = feed_urls.get(usize::from(next)) {
        return Some(pod0_storage::LibraryNetworkObservationAction::ContinueCatalog {
            step: LibraryNetworkStep::CatalogFeed {
                feed_urls: feed_urls.to_vec(),
                ordinal: next,
                candidates,
            },
            request: pod0_application::plan_shared_feed_request(feed_url)?,
        });
    }
    Some(pod0_storage::LibraryNetworkObservationAction::CompleteCatalog {
        candidates: pod0_application::select_catalog_candidates(candidates, *limit),
    })
}
