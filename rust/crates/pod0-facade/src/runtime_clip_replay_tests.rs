use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

fn envelope(id: u64, command: ApplicationCommand) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(70, id),
        cancellation_id: CancellationId::from_parts(71, id),
        expected_revision: None,
        command,
    }
}

fn projection(facade: &Pod0Facade) -> ClipsProjection {
    let Projection::Clips { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Clips {
                scope: ClipProjectionScope::All,
            },
            offset: 0,
            max_items: 100,
        })
        .projection
    else {
        panic!("expected clips projection");
    };
    value
}

#[test]
fn replay_after_later_mutation_returns_the_original_clip_and_collection_revisions() {
    let fixture = PlaybackFixture::new();
    let clip_id = ClipId::from_parts(72, 1);
    let create = envelope(
        1,
        ApplicationCommand::CreateClip {
            clip_id,
            episode_id: fixture.episode_id,
            podcast_id: fixture.podcast_id,
            start_milliseconds: 1_000,
            end_milliseconds: 2_000,
            caption: None,
            speaker_id: None,
            frozen_transcript_text: "Original".to_owned(),
            source: ClipSource::Touch,
        },
    );
    fixture.facade.dispatch(create.clone());
    let original_collection = projection(&fixture.facade).collection_revision;
    let update = envelope(
        2,
        ApplicationCommand::UpdateClip {
            clip_id,
            expected_clip_revision: ClipRevision::INITIAL,
            start_milliseconds: 1_100,
            end_milliseconds: 2_100,
            caption: None,
            speaker_id: None,
            frozen_transcript_text: "Updated".to_owned(),
        },
    );
    fixture.facade.dispatch(update.clone());
    let updated_collection = projection(&fixture.facade).collection_revision;
    fixture.facade.dispatch(envelope(
        3,
        ApplicationCommand::SetClipDeleted {
            clip_id,
            expected_clip_revision: ClipRevision::new(2),
            deleted: true,
        },
    ));

    let relaunched = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    relaunched.dispatch(create);
    relaunched.dispatch(update);
    let replayed = projection(&relaunched);
    assert_eq!(replayed.clips[0].revision, ClipRevision::new(3));
    let create_result = replayed
        .operations
        .iter()
        .find(|operation| operation.command_id == CommandId::from_parts(70, 1))
        .and_then(|operation| operation.result);
    assert!(matches!(
        create_result,
        Some(OperationResult::ClipCreated {
            clip_revision: ClipRevision { value: 1 },
            collection_revision,
            ..
        }) if collection_revision == original_collection
    ));
    let update_result = replayed
        .operations
        .iter()
        .find(|operation| operation.command_id == CommandId::from_parts(70, 2))
        .and_then(|operation| operation.result);
    assert!(matches!(
        update_result,
        Some(OperationResult::ClipUpdated {
            clip_revision: ClipRevision { value: 2 },
            collection_revision,
            ..
        }) if collection_revision == updated_collection
    ));
}

#[test]
fn deletion_and_clear_replays_keep_their_original_outcomes_after_later_commands() {
    let fixture = PlaybackFixture::new();
    let clip_id = ClipId::from_parts(73, 1);
    fixture.facade.dispatch(envelope(
        10,
        ApplicationCommand::CreateClip {
            clip_id,
            episode_id: fixture.episode_id,
            podcast_id: fixture.podcast_id,
            start_milliseconds: 3_000,
            end_milliseconds: 4_000,
            caption: None,
            speaker_id: None,
            frozen_transcript_text: "Replay deletion".to_owned(),
            source: ClipSource::Auto,
        },
    ));
    let delete = envelope(
        11,
        ApplicationCommand::SetClipDeleted {
            clip_id,
            expected_clip_revision: ClipRevision::INITIAL,
            deleted: true,
        },
    );
    fixture.facade.dispatch(delete.clone());
    let deleted_collection = projection(&fixture.facade).collection_revision;
    fixture.facade.dispatch(envelope(
        12,
        ApplicationCommand::SetClipDeleted {
            clip_id,
            expected_clip_revision: ClipRevision::new(2),
            deleted: false,
        },
    ));
    let clear = envelope(
        13,
        ApplicationCommand::ClearClips {
            expected_collection_revision: projection(&fixture.facade).collection_revision,
        },
    );
    fixture.facade.dispatch(clear.clone());
    let cleared_collection = projection(&fixture.facade).collection_revision;
    fixture.facade.dispatch(envelope(
        14,
        ApplicationCommand::CreateClip {
            clip_id: ClipId::from_parts(73, 2),
            episode_id: fixture.episode_id,
            podcast_id: fixture.podcast_id,
            start_milliseconds: 5_000,
            end_milliseconds: 6_000,
            caption: None,
            speaker_id: None,
            frozen_transcript_text: "Later command".to_owned(),
            source: ClipSource::Touch,
        },
    ));

    let relaunched = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    relaunched.dispatch(delete);
    relaunched.dispatch(clear);
    let replayed = projection(&relaunched);
    let result = |id| {
        replayed
            .operations
            .iter()
            .find(|operation| operation.command_id == CommandId::from_parts(70, id))
            .and_then(|operation| operation.result)
    };
    assert!(matches!(
        result(11),
        Some(OperationResult::ClipUpdated {
            clip_revision: ClipRevision { value: 2 },
            collection_revision,
            ..
        }) if collection_revision == deleted_collection
    ));
    assert!(matches!(
        result(13),
        Some(OperationResult::ClipsCleared { collection_revision })
            if collection_revision == cleared_collection
    ));
}
