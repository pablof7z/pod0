use crate::runtime_agent_modules::generated_audio_tests::{generated_episode, start};
use crate::runtime_agent_modules::tests::record_leased_agent_observation;
use crate::runtime_playback_test_support::PlaybackFixture;
use crate::*;
use pod0_application::{
    LeasedNMPPublicationObservation, LeasedNMPPublicationReceipt, PublicationStatusObservation,
};
use std::sync::Arc;

struct PublicationClock(i64);

impl pod0_application::Clock for PublicationClock {
    fn now(&self) -> UnixTimestampMilliseconds {
        UnixTimestampMilliseconds::new(self.0)
    }
}

#[test]
fn generated_episode_publication_hands_off_to_nmp_and_persists_receipt_across_restart() {
    let fixture = PlaybackFixture::new();
    let (_, capability) = start(&fixture, 701);
    let HostRequest::ExecuteAgentCapability {
        capability: request,
    } = &capability.request.request
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
    record_leased_agent_observation(
        &fixture.facade,
        &capability,
        HostObservation::AgentCapabilityObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal_id,
            execution_fence_id: request.execution_fence_id,
            outcome: AgentCapabilityOutcome::GeneratedAudioStaged {
                evidence: evidence.clone(),
            },
        },
    );
    let episode = generated_episode(&fixture.facade);
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

    let drafts = fixture.facade.next_nmp_publications(8);
    assert_eq!(drafts.len(), 1);
    let leased = &drafts[0];
    let draft = &leased.draft;
    assert_eq!(draft.expected_author_hex, "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798");
    assert_eq!(draft.kind, POD0_PODCAST_EPISODE_KIND);
    assert!(fixture.facade.next_nmp_publications(8).is_empty());

    fixture
        .facade
        .record_nmp_publication_receipt(LeasedNMPPublicationReceipt {
            lease: leased.lease,
            publication_id: draft.publication_id,
            receipt_id: 904,
        });
    fixture.facade.record_nmp_publication_observation(
        LeasedNMPPublicationObservation {
            lease: leased.lease,
            publication_id: draft.publication_id,
            observation: PublicationStatusObservation {
            kind: PublicationFactKind::Acknowledged,
            route_id: None,
            attempt: Some(1),
            event_id_hex: Some(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ),
            observed_at: None,
            detail: Some("NMP relay acknowledgement".into()),
        },
        },
    );

    let Projection::Publications { value } = fixture.facade.snapshot(ProjectionRequest {
        scope: ProjectionScope::Publications { publication_id: Some(draft.publication_id) },
        offset: 0,
        max_items: 10,
    }).projection else {
        panic!("expected publications");
    };
    let record = &value.items[0];
    assert_eq!(record.episode_id, episode.episode_id);
    assert_eq!(record.receipt_id, Some(904));
    assert_eq!(record.stage, PublicationStage::Acknowledged);
    assert_eq!(record.facts[0].kind, PublicationFactKind::Acknowledged);
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
    assert_eq!(value.items[0].stage, PublicationStage::Acknowledged);
    drop(_directory);
}

#[test]
fn publication_lease_reissues_after_expiry_and_rejects_the_stale_fence() {
    let fixture = PlaybackFixture::new();
    let (_, capability) = start(&fixture, 711);
    let HostRequest::ExecuteAgentCapability { capability: request } = &capability.request.request
    else {
        panic!("expected capability")
    };
    let target = request.generated_audio_target.unwrap();
    record_leased_agent_observation(
        &fixture.facade,
        &capability,
        HostObservation::AgentCapabilityObserved {
            turn_id: request.turn_id,
            proposal_id: request.proposal_id,
            execution_fence_id: request.execution_fence_id,
            outcome: AgentCapabilityOutcome::GeneratedAudioStaged {
                evidence: AgentGeneratedAudioEvidence {
                    artifact_id: target.artifact_id,
                    file_url: "file:///private/agent/restart.mp3".into(),
                    media_type: "audio/mpeg".into(),
                    byte_count: 1_024,
                    content_digest: ContentDigest::from_bytes([72; 32]),
                    duration_milliseconds: None,
                },
            },
        },
    );
    fixture.facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(712, 1),
        cancellation_id: CancellationId::from_parts(713, 1),
        expected_revision: None,
        command: ApplicationCommand::PublishGeneratedEpisode {
            intent: PublicationIntent {
                artifact_id: target.artifact_id,
                kind: PublicationArtifactKind::GeneratedPodcastEpisode,
                expected_author_hex: "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".into(),
                semantic_revision: 2,
                media: PublicationMediaEvidence {
                    public_url: "https://media.example/restart.mp3".into(),
                    media_type: "audio/mpeg".into(),
                    byte_count: 1_024,
                    content_digest: ContentDigest::from_bytes([72; 32]),
                },
            },
        },
    });
    let stale = fixture.facade.next_nmp_publications(1).remove(0);
    let path = fixture.target.to_string_lossy().into_owned();
    let clock = Arc::new(PublicationClock(stale.lease.expires_at.value + 1));
    drop(fixture.facade);
    let reopened = Pod0Facade::open_with_clock(path, clock);
    let fresh = reopened.next_nmp_publications(1).remove(0);
    assert!(fresh.lease.fence > stale.lease.fence);
    reopened.record_nmp_publication_receipt(LeasedNMPPublicationReceipt {
        lease: stale.lease,
        publication_id: stale.draft.publication_id,
        receipt_id: 1,
    });
    let Projection::Publications { value } = reopened
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Publications {
                publication_id: Some(stale.draft.publication_id),
            },
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected publications")
    };
    assert_eq!(value.items[0].receipt_id, None);
    reopened.record_nmp_publication_receipt(LeasedNMPPublicationReceipt {
        lease: fresh.lease,
        publication_id: fresh.draft.publication_id,
        receipt_id: 2,
    });
    assert_eq!(reopened.nmp_publication_receipt_links()[0].receipt_id, 2);
}
