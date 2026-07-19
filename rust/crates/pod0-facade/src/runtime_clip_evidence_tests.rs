use crate::runtime_recall_test_support::{RecallFixture, evidence_input, evidence_policy};
use crate::*;

fn clips(facade: &Pod0Facade) -> ClipsProjection {
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

fn envelope(id: u64, command: ApplicationCommand) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(40, id),
        cancellation_id: CancellationId::from_parts(41, id),
        expected_revision: None,
        command,
    }
}

#[test]
fn clip_evidence_survives_replacement_restart_and_unsubscribe_until_bounds_change() {
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
    let first = clips(&fixture.base.facade).clips[0].clone();
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
    let captioned = clips(&fixture.base.facade).clips[0].clone();
    assert_eq!(captioned.evidence, Some(first_evidence));

    fixture.base.facade.dispatch(envelope(
        22,
        ApplicationCommand::Unsubscribe {
            podcast_id: fixture.base.podcast_id,
        },
    ));
    let reopened = Pod0Facade::open(fixture.base.target.to_string_lossy().into_owned()).unwrap();
    let recovered = clips(&reopened).clips[0].clone();
    assert_eq!(recovered.evidence, Some(first_evidence));
    assert_eq!(recovered.frozen_transcript_text, "Evidence one");
    let reopened_store = pod0_storage::EvidenceStore::open(&fixture.base.target).unwrap();
    assert!(
        reopened_store
            .generation(fixture.artifact.generation_id)
            .unwrap()
            .is_some()
    );

    reopened.dispatch(envelope(
        23,
        ApplicationCommand::UpdateClip {
            clip_id,
            expected_clip_revision: recovered.revision,
            start_milliseconds: 14_500,
            end_milliseconds: 15_500,
            caption: recovered.caption,
            speaker_id: recovered.speaker_id,
            frozen_transcript_text: recovered.frozen_transcript_text,
        },
    ));
    let moved = clips(&reopened).clips[0].clone();
    assert_eq!(
        moved.evidence.map(|value| value.generation_id),
        Some(next.generation_id)
    );
    let relaunched = Pod0Facade::open(fixture.base.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(clips(&relaunched).clips[0], moved);
    assert!(
        reopened_store
            .generation(next.generation_id)
            .unwrap()
            .is_some()
    );
}
