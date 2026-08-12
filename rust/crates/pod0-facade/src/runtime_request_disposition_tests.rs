use pod0_application::{ActivityFact, RequestDisposition, RequestRejectionReason};
use pod0_storage::ActivityStore;

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[test]
fn boundary_rejections_are_durable_without_fake_state_transitions() {
    let fixture = PlaybackFixture::new();
    let missing = CommandEnvelope {
        command_id: CommandId::from_parts(81, 1),
        cancellation_id: CancellationId::from_parts(82, 1),
        expected_revision: None,
        command: ApplicationCommand::RequestPlayback {
            episode_id: fixture.episode_id,
        },
    };
    fixture.facade.dispatch(missing.clone());
    let episode = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 20)
        .unwrap();
    assert!(has_rejection(
        &episode.items,
        missing.command_id,
        RequestRejectionReason::MissingSubject,
    ));
    assert!(
        !episode
            .items
            .iter()
            .any(|item| matches!(item.draft.fact, ActivityFact::DomainTransition { .. }))
    );

    let unsupported = CommandEnvelope {
        command_id: CommandId::from_parts(81, 2),
        cancellation_id: CancellationId::from_parts(82, 2),
        expected_revision: None,
        command: ApplicationCommand::Unsupported { wire_code: 77 },
    };
    fixture.facade.dispatch(unsupported.clone());
    let global = ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_correlation(
            pod0_application::CommandActivityIdentity::new(unsupported.command_id).correlation_id(),
            None,
            20,
        )
        .unwrap();
    assert!(has_rejection(
        &global.items,
        unsupported.command_id,
        RequestRejectionReason::UnsupportedCode { wire_code: 77 },
    ));
}

fn has_rejection(
    facts: &[pod0_application::CommittedActivityFact],
    command_id: CommandId,
    expected: RequestRejectionReason,
) -> bool {
    facts.iter().any(|item| {
        matches!(
            item.draft.fact,
            ActivityFact::RequestDisposition {
                disposition: RequestDisposition::Rejected { reason },
            } if reason == expected && item.draft.command_id == Some(command_id)
        )
    })
}
