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

    fn last(&self) -> ProjectionEnvelope {
        self.projections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last()
            .cloned()
            .expect("subscriber must have received a projection")
    }
}

impl ProjectionSubscriber for RecordingSubscriber {
    fn receive(&self, projection: ProjectionEnvelope) {
        self.projections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(projection);
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
        offset: 0,
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
    assert!(!subscriber.last().content_changed);

    facade.unsubscribe(handle);
    facade.dispatch(command(
        2,
        20,
        ApplicationCommand::Unsupported { wire_code: 78 },
    ));
    assert_eq!(subscriber.count(), 2);
}

#[test]
fn subscription_does_not_redeliver_an_unchanged_projection() {
    let facade = Pod0Facade::new();
    let subscriber = std::sync::Arc::new(RecordingSubscriber::default());
    facade.subscribe(
        ProjectionRequest {
            scope: ProjectionScope::Playback,
            offset: 0,
            max_items: 1,
        },
        subscriber.clone(),
    );
    assert_eq!(subscriber.count(), 1);

    facade.dispatch(command(
        1,
        10,
        ApplicationCommand::Unsupported { wire_code: 77 },
    ));

    assert_eq!(subscriber.count(), 1);
}

#[test]
fn library_subscription_detects_changes_beyond_its_bounded_page() {
    use crate::runtime_playback_test_support::PlaybackFixture;

    let fixture = PlaybackFixture::new();
    {
        let mut state = fixture.facade.state();
        let template = state
            .listening
            .episodes
            .first()
            .cloned()
            .expect("fixture episode");
        for ordinal in 1..25_u64 {
            let mut episode = template.clone();
            episode.episode_id = EpisodeId::from_parts(91, ordinal);
            episode.publisher_guid = format!("page-two-{ordinal}");
            episode.title = format!("Page two episode {ordinal}");
            state.listening.episodes.push(episode);
        }
    }

    let subscriber = std::sync::Arc::new(RecordingSubscriber::default());
    fixture
        .facade
        .subscribe(library_request(), subscriber.clone());
    assert_eq!(subscriber.count(), 1);

    let was_starred = fixture.facade.state().listening.episodes[24].is_starred;
    fixture.facade.state().listening.episodes[24].is_starred = !was_starred;
    fixture.facade.notify_subscribers();

    assert_eq!(subscriber.count(), 2);
    assert!(subscriber.last().content_changed);
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
