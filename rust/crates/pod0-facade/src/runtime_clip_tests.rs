use crate::runtime_playback_test_support::PlaybackFixture;
use crate::runtime_recall_test_support::{RecallFixture, evidence_input, evidence_policy};
use crate::*;

fn clips(facade: &Pod0Facade, scope: ClipProjectionScope) -> ClipsProjection {
    let Projection::Clips { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Clips { scope },
            offset: 0,
            max_items: 100,
        })
        .projection
    else {
        panic!("expected clips projection");
    };
    value
}

fn envelope(id: u64, command: ApplicationCommand) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(40, id),
        cancellation_id: CancellationId::from_parts(41, id),
        expected_revision: None,
        command,
    }
}

fn operation(projection: &ClipsProjection, id: u64) -> &OperationProjection {
    projection
        .operations
        .iter()
        .find(|operation| operation.command_id == CommandId::from_parts(40, id))
        .expect("clip operation should be projected")
}

fn create(fixture: &PlaybackFixture, id: u64, clip_id: ClipId) -> CommandEnvelope {
    envelope(
        id,
        ApplicationCommand::CreateClip {
            clip_id,
            episode_id: fixture.episode_id,
            podcast_id: fixture.podcast_id,
            start_milliseconds: 10_000,
            end_milliseconds: 12_000,
            caption: Some("A durable moment".to_owned()),
            speaker_id: Some(SpeakerId::from_parts(5, 6)),
            frozen_transcript_text: "Exact frozen words".to_owned(),
            source: ClipSource::Touch,
        },
    )
}

#[test]
fn clip_commands_are_single_writer_revision_checked_and_restart_durable() {
    let fixture = PlaybackFixture::new();
    let clip_id = ClipId::from_parts(7, 8);
    fixture.facade.dispatch(create(&fixture, 1, clip_id));

    let created = clips(&fixture.facade, ClipProjectionScope::All);
    assert_eq!(created.clips.len(), 1);
    assert_eq!(created.clips[0].clip_id, clip_id);
    assert_eq!(created.clips[0].revision, ClipRevision::INITIAL);
    assert!(matches!(
        operation(&created, 1).result,
        Some(OperationResult::ClipCreated {
            clip_id: id,
            clip_revision: ClipRevision { value: 1 },
            collection_revision,
        }) if id == clip_id && collection_revision == created.collection_revision
    ));

    fixture.facade.dispatch(envelope(
        2,
        ApplicationCommand::UpdateClip {
            clip_id,
            expected_clip_revision: ClipRevision::INITIAL,
            start_milliseconds: 10_500,
            end_milliseconds: 12_500,
            caption: Some("Refined".to_owned()),
            speaker_id: None,
            frozen_transcript_text: "Refined exact words".to_owned(),
        },
    ));
    let updated = clips(
        &fixture.facade,
        ClipProjectionScope::Episode {
            episode_id: fixture.episode_id,
        },
    );
    assert_eq!(updated.clips[0].revision, ClipRevision::new(2));
    assert_eq!(updated.clips[0].start_milliseconds, 10_500);
    assert_eq!(updated.clips[0].caption.as_deref(), Some("Refined"));
    assert_eq!(
        clips(&fixture.facade, ClipProjectionScope::Clip { clip_id },).clips,
        updated.clips
    );

    fixture.facade.dispatch(envelope(
        3,
        ApplicationCommand::UpdateClip {
            clip_id,
            expected_clip_revision: ClipRevision::INITIAL,
            start_milliseconds: 0,
            end_milliseconds: 1,
            caption: None,
            speaker_id: None,
            frozen_transcript_text: "stale".to_owned(),
        },
    ));
    let conflicted = clips(&fixture.facade, ClipProjectionScope::All);
    assert_eq!(conflicted.clips[0].start_milliseconds, 10_500);
    assert!(matches!(
        operation(&conflicted, 3).failure,
        Some(CoreFailure {
            code: CoreFailureCode::RevisionConflict,
            ..
        })
    ));

    fixture.facade.dispatch(envelope(
        4,
        ApplicationCommand::SetClipDeleted {
            clip_id,
            expected_clip_revision: ClipRevision::new(2),
            deleted: true,
        },
    ));
    assert!(
        clips(&fixture.facade, ClipProjectionScope::Active)
            .clips
            .is_empty()
    );
    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let recovered = clips(&reopened, ClipProjectionScope::All);
    assert!(recovered.clips[0].deleted);
    assert_eq!(recovered.clips[0].revision, ClipRevision::new(3));

    reopened.dispatch(envelope(
        5,
        ApplicationCommand::ClearClips {
            expected_collection_revision: recovered.collection_revision,
        },
    ));
    assert!(matches!(
        operation(&clips(&reopened, ClipProjectionScope::All), 5).result,
        Some(OperationResult::ClipsCleared { collection_revision })
            if collection_revision.value > recovered.collection_revision.value
    ));

    reopened.dispatch(envelope(
        6,
        ApplicationCommand::Unsubscribe {
            podcast_id: fixture.podcast_id,
        },
    ));
    let after_unsubscribe = clips(&reopened, ClipProjectionScope::All);
    assert_eq!(after_unsubscribe.clips.len(), 1);
    assert!(after_unsubscribe.clips[0].deleted);
    let relaunched = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    let recovered_clip = clips(&relaunched, ClipProjectionScope::All);
    assert_eq!(recovered_clip.clips, after_unsubscribe.clips);
    let Projection::Library { value } = relaunched
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 100,
        })
        .projection
    else {
        panic!("expected library projection");
    };
    assert!(value.podcasts.is_empty());
}

#[test]
fn clip_validation_and_command_replay_have_typed_deterministic_outcomes() {
    let fixture = PlaybackFixture::new();
    let clip_id = ClipId::from_parts(9, 10);
    fixture.facade.dispatch(envelope(
        10,
        ApplicationCommand::CreateClip {
            clip_id,
            episode_id: fixture.episode_id,
            podcast_id: fixture.podcast_id,
            start_milliseconds: 20,
            end_milliseconds: 20,
            caption: None,
            speaker_id: None,
            frozen_transcript_text: String::new(),
            source: ClipSource::Auto,
        },
    ));
    let invalid = clips(&fixture.facade, ClipProjectionScope::All);
    assert!(invalid.clips.is_empty());
    assert!(matches!(
        operation(&invalid, 10).failure,
        Some(CoreFailure {
            code: CoreFailureCode::InvalidClip,
            ..
        })
    ));

    let command = create(&fixture, 11, clip_id);
    fixture.facade.dispatch(command.clone());
    let first = clips(&fixture.facade, ClipProjectionScope::All);
    let reopened = Pod0Facade::open(fixture.target.to_string_lossy().into_owned()).unwrap();
    reopened.dispatch(command);
    let replayed = clips(&reopened, ClipProjectionScope::All);
    assert_eq!(replayed.clips.len(), 1);
    assert_eq!(replayed.collection_revision, first.collection_revision);
}

#[test]
fn clip_evidence_is_captured_and_only_retargeted_when_bounds_change() {
    let fixture = RecallFixture::new(true);
    let clip_id = ClipId::from_parts(11, 12);
    fixture.base.facade.dispatch(envelope(
        20,
        ApplicationCommand::CreateClip {
            clip_id,
            episode_id: fixture.base.episode_id,
            podcast_id: fixture.base.podcast_id,
            start_milliseconds: 15_000,
            end_milliseconds: 16_000,
            caption: None,
            speaker_id: None,
            frozen_transcript_text: "Evidence one".to_owned(),
            source: ClipSource::Agent,
        },
    ));
    let first = clips(&fixture.base.facade, ClipProjectionScope::All).clips[0].clone();
    let first_evidence = first.evidence.expect("selected span should cover clip");
    assert_eq!(first_evidence.generation_id, fixture.artifact.generation_id);

    let mut next_input = evidence_input(&fixture.base);
    next_input.source_revision = "clip-fixture-v2".to_owned();
    next_input.source_payload_digest = ContentDigest::from_bytes([0x88; 32]);
    next_input.segments[0].text.push_str(" Updated.");
    let next = pod0_application::build_evidence_artifact(&next_input, evidence_policy()).unwrap();
    let store = pod0_storage::EvidenceStore::open(&fixture.base.target).unwrap();
    store
        .stage_artifact(CommandId::from_parts(80, 1), &next, 1_800_000_002_000)
        .unwrap();
    store
        .verify_generation(
            CommandId::from_parts(80, 2),
            next.generation_id,
            1_800_000_002_001,
        )
        .unwrap();
    store
        .select_generation(
            CommandId::from_parts(80, 3),
            fixture.base.episode_id,
            next.generation_id,
            1_800_000_002_002,
        )
        .unwrap();

    fixture.base.facade.dispatch(envelope(
        21,
        ApplicationCommand::UpdateClip {
            clip_id,
            expected_clip_revision: first.revision,
            start_milliseconds: first.start_milliseconds,
            end_milliseconds: first.end_milliseconds,
            caption: Some("Caption only".to_owned()),
            speaker_id: first.speaker_id,
            frozen_transcript_text: first.frozen_transcript_text,
        },
    ));
    let captioned = clips(&fixture.base.facade, ClipProjectionScope::All).clips[0].clone();
    assert_eq!(captioned.evidence, Some(first_evidence));

    fixture.base.facade.dispatch(envelope(
        22,
        ApplicationCommand::UpdateClip {
            clip_id,
            expected_clip_revision: captioned.revision,
            start_milliseconds: 14_500,
            end_milliseconds: 15_500,
            caption: captioned.caption,
            speaker_id: captioned.speaker_id,
            frozen_transcript_text: captioned.frozen_transcript_text,
        },
    ));
    assert_eq!(
        clips(&fixture.base.facade, ClipProjectionScope::All).clips[0]
            .evidence
            .map(|value| value.generation_id),
        Some(next.generation_id)
    );
}
