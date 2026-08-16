use pod0_domain::{CancellationId, CommandId, HostRequestId, PodcastId, StateRevision};

use crate::{
    DurableFeedEffectAction, DurableFeedEffectRequest, FeedFetchActivityInput, RequestDisposition,
    plan_feed_fetch,
};

#[test]
fn admission_couples_feed_transition_and_exact_effect() {
    let command_id = CommandId::from_parts(0, 1);
    let podcast_id = PodcastId::from_parts(0, 2);
    let plan = plan_feed_fetch(FeedFetchActivityInput {
        command_id,
        podcast_id,
        current_revision: StateRevision::new(4),
        legacy_command_revision: None,
        semantic_change: true,
        effect: Some(DurableFeedEffectRequest {
            request_id: HostRequestId::from_parts(0, 3),
            command_id,
            cancellation_id: CancellationId::from_parts(0, 4),
            issued_revision: StateRevision::new(5),
            not_before: None,
            deadline_at: None,
            action: DurableFeedEffectAction::FetchFeed {
                podcast_id,
                feed_url: "https://example.test/feed".to_owned(),
                entity_tag: None,
                last_modified: None,
            },
        }),
    })
    .expect("plan feed fetch");
    let (_, _, _, facts, effects, _, disposition) = plan.into_parts();
    assert_eq!(disposition, RequestDisposition::Accepted);
    assert_eq!(facts.len(), 3);
    assert_eq!(effects.len(), 1);
}
