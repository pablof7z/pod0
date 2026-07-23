use std::sync::{Arc, mpsc};
use std::time::Duration;

use crate::runtime_agent_modules::generated_audio_tests::{generated_episode, observe, start};
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;

struct Subscriber(mpsc::Sender<ProjectionEnvelope>);

impl ProjectionSubscriber for Subscriber {
    fn receive(&self, projection: ProjectionEnvelope) {
        let _ = self.0.send(projection);
    }
}

fn publication(envelope: ProjectionEnvelope) -> Option<PublicationRecord> {
    let Projection::Publications { value } = envelope.projection else {
        return None;
    };
    value.items.into_iter().next()
}

#[test]
fn generated_episode_publication_persists_receipt_and_missing_signer_across_restart() {
    let fixture = PlaybackFixture::new();
    let (_, capability) = start(&fixture, 701);
    let HostRequest::ExecuteAgentCapability {
        capability: request,
    } = &capability.request
    else {
        panic!("expected capability");
    };
    let target = request.generated_audio_target.unwrap();
    let evidence = AgentGeneratedAudioEvidence {
        artifact_id: target.artifact_id,
        file_url: "file:///private/agent/publishable.mp3".into(),
        media_type: "audio/mpeg".into(),
        byte_count: 8_192,
        content_digest: ContentDigest::from_bytes([71; 32]),
        duration_milliseconds: Some(45_000),
    };
    fixture.facade.record_host_observation(observe(
        &capability,
        HostObservation::AgentCapabilityObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal_id,
            execution_fence_id: request.execution_fence_id,
            outcome: AgentCapabilityOutcome::GeneratedAudioStaged {
                evidence: evidence.clone(),
            },
        },
    ));
    let episode = generated_episode(&fixture.facade);
    let (sender, receiver) = mpsc::channel();
    let subscriber = Arc::new(Subscriber(sender));
    fixture.facade.subscribe(
        ProjectionRequest {
            scope: ProjectionScope::Publications {
                publication_id: None,
            },
            offset: 0,
            max_items: 10,
        },
        subscriber,
    );
    let _ = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(702, 1),
        cancellation_id: CancellationId::from_parts(703, 1),
        expected_revision: None,
        command: ApplicationCommand::PublishGeneratedEpisode {
            intent: PublicationIntent {
                artifact_id: target.artifact_id,
                kind: PublicationArtifactKind::GeneratedPodcastEpisode,
                expected_author_hex:
                    "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".into(),
                semantic_revision: 1,
                media: PublicationMediaEvidence {
                    public_url: "https://media.example/publishable.mp3".into(),
                    media_type: "audio/mpeg".into(),
                    byte_count: evidence.byte_count,
                    content_digest: evidence.content_digest,
                },
            },
        },
    });

    let mut latest = None;
    for _ in 0..6 {
        let envelope = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        if let Some(record) = publication(envelope) {
            let done = record.stage == PublicationStage::AwaitingCapability;
            latest = Some(record);
            if done {
                break;
            }
        }
    }
    let record = latest.expect("publication projection");
    assert_eq!(record.episode_id, episode.episode_id);
    assert!(record.receipt_id.is_some());
    assert_eq!(record.stage, PublicationStage::AwaitingCapability);
    assert_eq!(
        record
            .facts
            .iter()
            .map(|fact| fact.kind)
            .collect::<Vec<_>>(),
        vec![
            PublicationFactKind::Accepted,
            PublicationFactKind::AwaitingCapability
        ]
    );
    assert!(record.facts.iter().all(|fact| fact.route_id.is_none()));
    let publication_id = record.publication_id;
    let receipt_id = record.receipt_id;
    let PlaybackFixture {
        _directory,
        target: target_path,
        facade,
        ..
    } = fixture;
    drop(facade);

    let reopened = Pod0Facade::open(target_path.to_string_lossy().into_owned()).unwrap();
    let Projection::Publications { value } = reopened
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Publications {
                publication_id: Some(publication_id),
            },
            offset: 0,
            max_items: 10,
        })
        .projection
    else {
        panic!("expected publications");
    };
    assert_eq!(value.items.len(), 1);
    assert_eq!(value.items[0].receipt_id, receipt_id);
    assert_eq!(value.items[0].stage, PublicationStage::AwaitingCapability);
    drop(_directory);
}
