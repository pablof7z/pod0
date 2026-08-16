//! Claims about the bounded work queue the native host drains: how much
//! feed-fetch work one command produced, what work must no longer exist,
//! and which accepted work the host was told to stop.

use cucumber::then;

use pod0_application::HostRequest;

use crate::world::PodWorld;

#[then(regex = r#"^exactly one feed fetch reaches the host for "([^"]+)"$"#)]
async fn exactly_one_fetch(w: &mut PodWorld, url: String) {
    nothing_to_observe!(
        w.has_subscribed_to(&url),
        "the app never subscribed to {url:?}, so no fetch work could exist to count"
    );
    let drained = w.drain_all_host_requests();
    let fetches = PodWorld::feed_fetches_for(&drained, &url);
    assert_eq!(
        fetches,
        1,
        "expected exactly one feed fetch for {url:?}; the drained queue held {fetches} \
         among {} requests",
        drained.len()
    );
}

#[then(regex = r#"^no feed fetch work remains for the host$"#)]
async fn no_fetch_work_remains(w: &mut PodWorld) {
    nothing_to_observe!(
        w.is_started(),
        "no core ever ran in this scenario, so an empty work queue proves nothing"
    );
    let drained = w.drain_all_host_requests();
    let fetches: Vec<&str> = drained
        .iter()
        .filter_map(|request| match &request.request.request {
            HostRequest::FetchFeed { feed_url, .. } => Some(feed_url.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        fetches.is_empty(),
        "expected no remaining feed fetch work; the queue still held fetches for {fetches:?}"
    );
}

#[then(regex = r#"^the host is told to abandon the accepted announcement$"#)]
async fn host_told_to_abandon_announcement(w: &mut PodWorld) {
    let accepted = w.accepted_announcement().cloned();
    nothing_to_observe!(
        accepted.is_some(),
        "the host never accepted an announcement, so no cancellation could target it"
    );
    let accepted = accepted.expect("guarded above");
    let cancellations = w.drain_all_host_requests();
    assert!(
        cancellations.iter().any(|cancellation| matches!(
            cancellation.request.request,
            HostRequest::CancelAuthorizedEffect { target_request_id }
                if target_request_id == accepted.request.request_id
        )),
        "expected a leased host cancellation for the accepted announcement; got {cancellations:?}"
    );
}
