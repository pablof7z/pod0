//! Claims about one command's lifecycle: the semantic `OperationStage` the
//! projection reports, the typed failure it carries, the typed receipt a
//! host observation earned, and what the state revision did. This family
//! owns the lookup from a scenario's feed URL to its projected operation.

use cucumber::then;

use pod0_application::{
    CoreFailureCode, HostObservationReceipt, OperationProjection, OperationStage,
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

#[then(regex = r#"^the subscription to "([^"]+)" was cancelled$"#)]
async fn subscription_cancelled(w: &mut PodWorld, url: String) {
    let operation = subscribe_operation(w, &url);
    assert_eq!(
        operation.stage,
        OperationStage::Cancelled,
        "expected the subscribe to {url:?} to be cancelled; got {operation:?}"
    );
}

#[then(regex = r#"^the subscription to "([^"]+)" failed because the feed was malformed$"#)]
async fn subscription_failed_malformed(w: &mut PodWorld, url: String) {
    let operation = subscribe_operation(w, &url);
    assert_eq!(
        operation.stage,
        OperationStage::Failed,
        "expected the subscribe to {url:?} to fail; got {operation:?}"
    );
    let failure = operation
        .failure
        .unwrap_or_else(|| panic!("a failed operation must carry its typed failure"));
    assert_eq!(
        failure.code,
        CoreFailureCode::FeedMalformed,
        "expected the typed FeedMalformed failure; got {failure:?}"
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
