//! Claims about the app-visible library: which podcasts and episodes it
//! shows and which it must never show — read either as the bounded snapshot
//! an opening screen requests or as the deliveries a live view received.

use cucumber::then;

use crate::world::PodWorld;

#[then(regex = r#"^the library lists the podcast "([^"]+)"$"#)]
async fn library_lists_podcast(w: &mut PodWorld, title: String) {
    let library = w.library();
    let titles: Vec<&str> = library
        .podcasts
        .iter()
        .map(|podcast| podcast.title.as_str())
        .collect();
    assert!(
        titles.contains(&title.as_str()),
        "expected the library to list the podcast {title:?}; it lists {titles:?}"
    );
}

#[then(regex = r#"^the library lists the episode "([^"]+)"$"#)]
async fn library_lists_episode(w: &mut PodWorld, title: String) {
    let library = w.library();
    let titles: Vec<&str> = library
        .episodes
        .iter()
        .map(|episode| episode.title.as_str())
        .collect();
    assert!(
        titles.contains(&title.as_str()),
        "expected the library to list the episode {title:?}; it lists {titles:?}"
    );
}

#[then(regex = r#"^the library does not list the podcast "([^"]+)"$"#)]
async fn library_does_not_list_podcast(w: &mut PodWorld, title: String) {
    nothing_to_observe!(
        w.is_started(),
        "no core ever ran in this scenario, so no library existed to keep {title:?} out of"
    );
    let library = w.library();
    assert!(
        !library
            .podcasts
            .iter()
            .any(|podcast| podcast.title == title),
        "expected the library to not list the podcast {title:?}"
    );
}

#[then(regex = r#"^the library lists no podcasts$"#)]
async fn library_lists_nothing(w: &mut PodWorld) {
    nothing_to_observe!(
        w.is_started(),
        "no core ever ran in this scenario, so there is no library to be empty"
    );
    let library = w.library();
    assert!(
        library.podcasts.is_empty(),
        "expected an empty library; it lists {:?}",
        library
            .podcasts
            .iter()
            .map(|podcast| podcast.title.as_str())
            .collect::<Vec<_>>()
    );
}

#[then(regex = r#"^the live library view received the podcast "([^"]+)"$"#)]
async fn live_view_received_podcast(w: &mut PodWorld, title: String) {
    let deliveries = w.library_watch_deliveries();
    nothing_to_observe!(
        deliveries.is_some(),
        "no live library view was ever opened, so nothing could have been delivered"
    );
    let deliveries = deliveries.expect("guarded above");
    assert!(
        deliveries.iter().any(|library| library
            .podcasts
            .iter()
            .any(|podcast| podcast.title == title)),
        "expected a live library delivery carrying the podcast {title:?}; \
         {} deliveries arrived without it",
        deliveries.len()
    );
}

#[then(regex = r#"^no further library deliveries arrived$"#)]
async fn no_further_deliveries(w: &mut PodWorld) {
    let at_close = w.deliveries_at_close();
    nothing_to_observe!(
        at_close.is_some(),
        "the live library view was never closed, so there is no moment for 'further' to start from"
    );
    let now = w
        .watch_delivery_count()
        .expect("a closed view still holds its recorder");
    assert_eq!(
        Some(now),
        at_close,
        "expected delivery count to stay at its close-time value"
    );
}
