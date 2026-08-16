use pod0_application::{
    ActivityDomain, ActivityFact, ActivityOrigin, ActivitySubject, ApplicationCommand,
    CommandEnvelope, DurableInternalCommandRequest, HostRequest, InternalCommandKind,
    PlaybackCommand, PlaybackTransition, Projection, ProjectionRequest, ProjectionScope,
    TranscriptProvider, TranscriptWorkflowConfiguration, TranscriptWorkflowOrigin,
};
use pod0_domain::{CancellationId, CommandId, TranscriptStartPolicy};

use crate::{Pod0Facade, runtime_playback_test_support::PlaybackFixture};

#[test]
fn automatic_policy_admits_background_transcript_work() {
    let fixture = PlaybackFixture::new();
    ensure_automatic(&fixture, 20);

    assert!(has_transcript_request(&fixture.facade));
    assert_eq!(transcript_workflow_count(&fixture.facade), 1);
}

#[test]
fn when_played_policy_defers_automatic_work_until_play_and_deduplicates_replays() {
    let fixture = PlaybackFixture::new();
    set_transcript_policy(&fixture, TranscriptStartPolicy::WhenPlayed, 30);
    ensure_automatic(&fixture, 31);

    assert!(!has_transcript_request(&fixture.facade));
    assert_eq!(transcript_workflow_count(&fixture.facade), 0);

    fixture.dispatch(
        32,
        PlaybackCommand::Play {
            transcript_configuration: Some(configuration()),
        },
    );
    assert!(has_transcript_request(&fixture.facade));
    assert_eq!(transcript_workflow_count(&fixture.facade), 1);

    fixture.dispatch(
        33,
        PlaybackCommand::Play {
            transcript_configuration: Some(configuration()),
        },
    );
    assert!(!has_transcript_request(&fixture.facade));
    assert_eq!(transcript_workflow_count(&fixture.facade), 1);
    let store = pod0_storage::LibraryStore::open_authoritative(&fixture.target).unwrap();
    assert!(store.pending_internal_commands(100).unwrap().is_empty());
    let activity = pod0_storage::ActivityStore::open(&fixture.target)
        .unwrap()
        .page_for_episode(fixture.episode_id, None, 100)
        .unwrap();
    assert!(activity.items.iter().any(|item| matches!(
        item.draft.fact,
        ActivityFact::InternalCommandAuthorized {
            target: ActivityDomain::Transcript,
            ..
        }
    )));
    assert!(activity.items.iter().any(|item| {
        item.draft.origin == ActivityOrigin::InternalCommand
            && item.draft.caused_by_activity_id.is_some()
            && matches!(item.draft.fact, ActivityFact::DomainTransition { .. })
    }));
}

#[test]
fn startup_resumes_an_authorized_playback_transcript_command() {
    let fixture = PlaybackFixture::new();
    set_transcript_policy(&fixture, TranscriptStartPolicy::WhenPlayed, 40);
    let command_id = CommandId::from_parts(80, 1);
    let request = DurableInternalCommandRequest {
        kind: InternalCommandKind::EnsureTranscriptWorkflow {
            origin: TranscriptWorkflowOrigin::Playback,
            configuration: configuration(),
        },
        target: ActivityDomain::Transcript,
        subject: ActivitySubject::Episode {
            episode_id: fixture.episode_id,
        },
        episode_id: Some(fixture.episode_id),
    };
    let store = pod0_storage::LibraryStore::open_authoritative(&fixture.target).unwrap();
    store
        .apply_playback_mutation(
            command_id,
            &"a".repeat(64),
            pod0_storage::PlaybackMutation::ReceiptOnly,
            Some(fixture.episode_id),
            PlaybackTransition::SessionStateChanged,
            Some(request),
            Vec::new(),
            1_800_000_000_100,
        )
        .unwrap();
    assert_eq!(store.pending_internal_commands(100).unwrap().len(), 1);

    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    assert!(has_transcript_request(&reopened));
    assert!(store.pending_internal_commands(100).unwrap().is_empty());
    assert_eq!(transcript_workflow_count(&reopened), 1);
}

fn set_transcript_policy(fixture: &PlaybackFixture, policy: TranscriptStartPolicy, command: u64) {
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(70, command),
        cancellation_id: CancellationId::from_parts(71, command),
        expected_revision: None,
        command: ApplicationCommand::SetSubscriptionTranscriptStartPolicy {
            podcast_id: fixture.podcast_id,
            policy,
        },
    });
}

fn ensure_automatic(fixture: &PlaybackFixture, command: u64) {
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(70, command),
        cancellation_id: CancellationId::from_parts(71, command),
        expected_revision: None,
        command: ApplicationCommand::EnsureTranscriptWorkflow {
            episode_id: fixture.episode_id,
            origin: TranscriptWorkflowOrigin::Automatic,
            configuration: configuration(),
        },
    });
}

fn has_transcript_request(facade: &Pod0Facade) -> bool {
    facade
        .next_leased_host_requests(u16::MAX)
        .into_iter()
        .any(|leased| {
            matches!(
                leased.request.request,
                HostRequest::ExecuteTranscriptCapability { .. }
            )
        })
}

fn transcript_workflow_count(facade: &Pod0Facade) -> usize {
    let Projection::TranscriptWorkflows { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::TranscriptWorkflows { episode_id: None },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected transcript workflow projection");
    };
    value.workflows.len()
}

fn configuration() -> TranscriptWorkflowConfiguration {
    TranscriptWorkflowConfiguration {
        provider: TranscriptProvider::AssemblyAi,
        model: "universal-2".into(),
        local_audio_url: None,
        credential_available: true,
        auto_publisher_enabled: true,
        auto_provider_enabled: true,
    }
}
