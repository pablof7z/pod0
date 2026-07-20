use pod0_application::Clock;

use crate::{
    ApplicationCommand, CancellationId, CommandEnvelope, CommandId, EpisodeId, KernelProbeCommand,
    KernelProbeFacade, MAX_HOST_REQUEST_BATCH, StateRevision, UnixTimestampMilliseconds,
    bounded_host_request_count, bounded_playback_observation_interval,
};

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(42)
    }
}

#[test]
fn facade_preserves_the_typed_application_projection() {
    let command = KernelProbeCommand {
        command_id: CommandId::from_bytes([4; 16]),
    };

    let projection = KernelProbeFacade::new(FixedClock).dispatch_probe(command);

    assert_eq!(projection.command_id, command.command_id);
    assert_eq!(projection.observed_at.value(), 42);
}

#[test]
fn listening_actions_are_typed_without_dynamic_dispatch() {
    let command = CommandEnvelope {
        command_id: CommandId::from_parts(0, 1),
        cancellation_id: CancellationId::from_parts(0, 2),
        expected_revision: Some(StateRevision::new(3)),
        command: ApplicationCommand::RequestPlayback {
            episode_id: EpisodeId::from_parts(0, 4),
        },
    };

    assert!(matches!(
        command.command,
        ApplicationCommand::RequestPlayback { episode_id }
            if episode_id == EpisodeId::from_parts(0, 4)
    ));
    assert_eq!(bounded_host_request_count(0), 1);
    assert_eq!(
        bounded_host_request_count(u16::MAX),
        usize::from(MAX_HOST_REQUEST_BATCH)
    );
    assert_eq!(bounded_playback_observation_interval(0), 500);
    assert_eq!(bounded_playback_observation_interval(1_000), 1_000);
    assert_eq!(bounded_playback_observation_interval(u32::MAX), 5_000);
}
