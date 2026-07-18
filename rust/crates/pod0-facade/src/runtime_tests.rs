use std::sync::Mutex;

use crate::*;

#[derive(Default)]
struct RecordingSubscriber {
    projections: Mutex<Vec<ProjectionEnvelope>>,
}

impl RecordingSubscriber {
    fn count(&self) -> usize {
        self.projections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

impl ProjectionSubscriber for RecordingSubscriber {
    fn receive(&self, projection: ProjectionEnvelope) -> Result<(), ProjectionDeliveryError> {
        self.projections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(projection);
        Ok(())
    }
}

fn command(command_id: u64, cancellation_id: u64, payload: ApplicationCommand) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(0, command_id),
        cancellation_id: CancellationId::from_parts(0, cancellation_id),
        expected_revision: None,
        command: payload,
    }
}

fn library_request() -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Library,
        max_items: 20,
    }
}

#[test]
fn subscription_is_event_driven_and_unsubscribe_stops_delivery() {
    let facade = Pod0Facade::new();
    let subscriber = std::sync::Arc::new(RecordingSubscriber::default());
    let handle = facade.subscribe(library_request(), subscriber.clone());
    assert_eq!(subscriber.count(), 1);

    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::Unsupported { wire_code: 77 },
    ));
    assert_eq!(subscriber.count(), 2);

    facade.unsubscribe(handle);
    facade.dispatch(command(
        2,
        20,
        ApplicationCommand::Unsupported { wire_code: 78 },
    ));
    assert_eq!(subscriber.count(), 2);
}

#[test]
fn cancellation_prevents_late_host_observation_from_committing() {
    let facade = Pod0Facade::new();
    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::SubscribeToFeed {
            feed_url: "https://example.test/feed".to_owned(),
        },
    ));
    let request = facade
        .next_host_requests(1)
        .into_iter()
        .next()
        .expect("subscribe command should issue one bounded host request");

    facade.dispatch(command(
        2,
        20,
        ApplicationCommand::CancelOperation {
            cancellation_id: CancellationId::from_parts(0, 10),
        },
    ));
    let revision_after_cancel = facade.snapshot(library_request()).state_revision;

    facade.record_host_observation(HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        observation: HostObservation::FeedBytesFetched {
            bytes: b"ignored late result".to_vec(),
            entity_tag: None,
            last_modified: None,
        },
    });

    let projection = facade.snapshot(library_request());
    assert_eq!(projection.state_revision, revision_after_cancel);
    let Projection::Library { value } = projection.projection else {
        panic!("expected library projection");
    };
    assert!(value.operations.iter().any(|operation| {
        operation.command_id == CommandId::from_parts(0, 1)
            && operation.stage == OperationStage::Cancelled
    }));
}

#[test]
fn host_request_drain_is_safe_when_limit_exceeds_queue_length() {
    let facade = Pod0Facade::new();
    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::SubscribeToFeed {
            feed_url: "https://example.test/feed".to_owned(),
        },
    ));

    assert_eq!(facade.next_host_requests(u16::MAX).len(), 1);
    assert!(facade.next_host_requests(u16::MAX).is_empty());
}

#[test]
fn cancellation_removes_native_work_that_has_not_started() {
    let facade = Pod0Facade::new();
    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::SubscribeToFeed {
            feed_url: "https://example.test/feed".to_owned(),
        },
    ));
    facade.dispatch(command(
        2,
        20,
        ApplicationCommand::CancelOperation {
            cancellation_id: CancellationId::from_parts(0, 10),
        },
    ));

    assert!(facade.next_host_requests(u16::MAX).is_empty());
}

#[test]
fn revision_conflict_is_terminal_for_the_command_identity() {
    let facade = Pod0Facade::new();
    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::Unsupported { wire_code: 1 },
    ));
    let stale = CommandEnvelope {
        command_id: CommandId::from_parts(0, 2),
        cancellation_id: CancellationId::from_parts(0, 20),
        expected_revision: Some(StateRevision::INITIAL),
        command: ApplicationCommand::Unsupported { wire_code: 2 },
    };
    facade.dispatch(stale.clone());
    let conflict_revision = facade.snapshot(library_request()).state_revision;

    facade.dispatch(stale);

    assert_eq!(
        facade.snapshot(library_request()).state_revision,
        conflict_revision
    );
}
