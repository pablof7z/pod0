use std::sync::Arc;

use rusqlite::Connection;

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

#[derive(Clone, Copy)]
pub(super) struct FixedScheduledClock(pub(super) i64);

impl pod0_application::Clock for FixedScheduledClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

pub(super) fn authoritative_fixture(now: i64) -> PlaybackFixture {
    let fixture = PlaybackFixture::new();
    let connection = Connection::open(&fixture.target).unwrap();
    let changed = connection
        .execute(
            "UPDATE pod0_scheduled_agent_authority SET state='authoritative',\
             source_generation=1,committed_at_ms=?1 WHERE singleton=1 AND state='inactive'",
            [now],
        )
        .unwrap();
    assert_eq!(changed, 1);
    fixture
}

pub(super) fn open_scheduled(fixture: &PlaybackFixture, now: i64) -> Arc<Pod0Facade> {
    Pod0Facade::open_with_clock(
        fixture.target.to_string_lossy().into_owned(),
        Arc::new(FixedScheduledClock(now)),
    )
}

pub(super) fn dispatch_scheduled(facade: &Pod0Facade, id: u64, command: ApplicationCommand) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(81, id),
        cancellation_id: CancellationId::from_parts(82, id),
        expected_revision: None,
        command,
    });
}

pub(super) fn task_input(next_run_at: i64) -> ScheduledTaskInput {
    ScheduledTaskInput {
        task_id: None,
        label: "Daily briefing".to_owned(),
        prompt: "Summarize my saved podcast evidence".to_owned(),
        model_reference: "openrouter:test/model".to_owned(),
        interval_milliseconds: 86_400_000,
        next_run_at: UnixTimestampMilliseconds::new(next_run_at),
    }
}

pub(super) fn scheduled_projection(facade: &Pod0Facade) -> ScheduledAgentProjection {
    let Projection::ScheduledAgent { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::ScheduledAgent { task_id: None },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected scheduled-agent projection")
    };
    value
}

pub(super) fn scheduled_observation(
    request: &HostRequestEnvelope,
    sequence_number: u64,
    observed_at: i64,
    observation: ScheduledAgentExecutionObservation,
) -> HostObservationEnvelope {
    HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number,
        observed_at: UnixTimestampMilliseconds::new(observed_at),
        observation: HostObservation::ScheduledAgentExecutionObserved { observation },
    }
}
