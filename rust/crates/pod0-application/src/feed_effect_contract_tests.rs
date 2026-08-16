use pod0_domain::{CancellationId, CommandId, HostRequestId, PodcastId, StateRevision};

use crate::{
    DurableFeedEffectAction, DurableFeedEffectRequest, HostRequest, MAX_FEED_RESPONSE_BYTES,
};

#[test]
fn fetch_effect_is_an_exact_reconstructable_host_request() {
    let request = DurableFeedEffectRequest {
        request_id: HostRequestId::from_parts(0, 1),
        command_id: CommandId::from_parts(0, 2),
        cancellation_id: CancellationId::from_parts(0, 3),
        issued_revision: StateRevision::new(4),
        not_before: None,
        deadline_at: None,
        action: DurableFeedEffectAction::FetchFeed {
            podcast_id: PodcastId::from_parts(0, 5),
            feed_url: "https://example.test/feed.xml".to_owned(),
            entity_tag: Some("etag".to_owned()),
            last_modified: None,
        },
    };
    assert!(matches!(
        request.to_host().request,
        HostRequest::FetchFeed {
            maximum_response_bytes: MAX_FEED_RESPONSE_BYTES,
            ..
        }
    ));
}
