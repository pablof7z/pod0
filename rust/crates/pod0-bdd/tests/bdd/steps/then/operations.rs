//! Claims about command acceptance and its durable feed-fetch workflow: the
//! projected stages and failure, the typed receipt a host observation earned,
//! and what the state revision did. This family owns the lookup from a
//! scenario's feed URL to both projected records.

use cucumber::then;

use pod0_application::{
    FeedFetchProjection, FeedFetchStage, HostObservationReceipt, OperationProjection,
    OperationStage,
};

use crate::world::PodWorld;

/// The shared lookup: the subscribe operation for `url` must exist in the
/// projection before any stage claim about it can mean anything.
fn subscribe_operation(w: &PodWorld, url: &str) -> OperationProjection {
    nothing_to_observe!(
        w.has_subscribed_to(url),
        "the app never subscribed to {url:?}, so no operation exists to make claims about"
    );
    w.subscribe_operation(url).unwrap_or_else(|| {
        panic!("expected the library projection to carry an operation for the subscribe to {url:?}")
    })
}

#[then(regex = r#"^the subscription to "([^"]+)" has succeeded$"#)]
async fn subscription_succeeded(w: &mut PodWorld, url: String) {
    let operation = subscribe_operation(w, &url);
    assert_eq!(
        operation.stage,
        OperationStage::Succeeded,
        "expected the subscribe to {url:?} to succeed; got {operation:?}"
    );
}

/// The durable feed-fetch workflow for `url` must exist before a stage claim
/// about it can mean anything.
fn feed_fetch(w: &PodWorld, url: &str) -> FeedFetchProjection {
    nothing_to_observe!(
        w.has_subscribed_to(url),
        "the app never subscribed to {url:?}, so no feed fetch exists to make claims about"
    );
    w.feed_fetch(url)
        .unwrap_or_else(|| panic!("expected a projected feed fetch for {url:?}"))
}

#[then(regex = r#"^the feed fetch for "([^"]+)" failed because the feed was malformed$"#)]
async fn feed_fetch_failed_malformed(w: &mut PodWorld, url: String) {
    let fetch = feed_fetch(w, &url);
    assert_eq!(
        fetch.stage,
        FeedFetchStage::Failed,
        "expected the feed fetch for {url:?} to fail; got {fetch:?}"
    );
    assert_eq!(
        fetch.failure_code.as_deref(),
        Some("feed_malformed"),
        "expected the durable feed workflow to carry feed_malformed; got {fetch:?}"
    );
}

#[then(regex = r#"^no feed fetch workflow remains for "([^"]+)"$"#)]
async fn no_feed_fetch_workflow_remains(w: &mut PodWorld, url: String) {
    nothing_to_observe!(
        w.has_subscribed_to(&url),
        "the app never subscribed to {url:?}, so an absent workflow proves nothing"
    );
    assert!(
        w.feed_fetch(&url).is_none(),
        "expected no durable feed fetch workflow for {url:?}"
    );
}

#[then(regex = r#"^the late feed bytes were refused$"#)]
async fn late_bytes_refused(w: &mut PodWorld) {
    let receipt = w.last_receipt();
    nothing_to_observe!(
        receipt.is_some(),
        "the host never reported an observation, so there is no receipt to have refused anything"
    );
    let receipt = receipt.expect("guarded above");
    assert!(
        matches!(receipt, HostObservationReceipt::Rejected { .. }),
        "expected the facade to reject the late observation; got {receipt:?}"
    );
}

#[then(regex = r#"^the state revision has not advanced since the cancellation$"#)]
async fn revision_unmoved_since_cancel(w: &mut PodWorld) {
    let marked = w.revision_at_cancel();
    nothing_to_observe!(
        marked.is_some(),
        "the app never cancelled anything, so there is no revision mark to measure from"
    );
    assert_eq!(
        Some(w.revision()),
        marked,
        "expected no state revision movement after the cancellation"
    );
}
