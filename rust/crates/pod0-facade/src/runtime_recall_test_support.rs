use pod0_application::{
    EvidenceChunkPolicy, TranscriptEvidenceInput, TranscriptSegmentInput, build_evidence_artifact,
};

use crate::runtime_playback_test_support::PlaybackFixture;
use crate::runtime_recall_configuration_test_support::enable_test_reranking;
use crate::*;

pub(super) struct RecallFixture {
    pub(super) base: PlaybackFixture,
    pub(super) artifact: pod0_domain::TranscriptEvidenceArtifact,
}

impl RecallFixture {
    pub(super) fn new(with_evidence: bool) -> Self {
        let base = PlaybackFixture::new();
        enable_test_reranking(&base.facade);
        let artifact = build_evidence_artifact(&evidence_input(&base), evidence_policy()).unwrap();
        assert!(artifact.spans.len() >= 2);
        if with_evidence {
            let store = pod0_storage::EvidenceStore::open(&base.target).unwrap();
            store
                .stage_artifact(CommandId::from_parts(20, 1), &artifact, 1_800_000_000_100)
                .unwrap();
            store
                .verify_generation(
                    CommandId::from_parts(20, 2),
                    artifact.generation_id,
                    1_800_000_000_101,
                )
                .unwrap();
            store
                .select_generation(
                    CommandId::from_parts(20, 3),
                    artifact.version.episode_id,
                    artifact.generation_id,
                    1_800_000_000_102,
                )
                .unwrap();
            base.facade.dispatch(CommandEnvelope {
                command_id: CommandId::from_parts(29, 1),
                cancellation_id: CancellationId::from_parts(29, 2),
                expected_revision: None,
                command: ApplicationCommand::RebuildTranscriptEvidence {
                    input: evidence_input(&base),
                    policy: evidence_policy(),
                },
            });
            complete_evidence_embedding_requests(&base.facade);
        }
        Self { base, artifact }
    }

    pub(super) fn dispatch(&self, id: u64, query_id: u64, text: &str) -> CommandEnvelope {
        let envelope = recall_command(
            id,
            query_id,
            text,
            RecallScope::Episode {
                episode_id: self.base.episode_id,
            },
            2,
        );
        self.base.facade.dispatch(envelope.clone());
        envelope
    }

    pub(super) fn projection(&self, query_id: u64) -> RecallResultProjection {
        recall_projection(&self.base.facade, query_id)
    }
}

pub(super) fn evidence_input(base: &PlaybackFixture) -> TranscriptEvidenceInput {
    TranscriptEvidenceInput {
        episode_id: base.episode_id,
        podcast_id: base.podcast_id,
        source_revision: "recall-fixture-v1".to_owned(),
        source: TranscriptSource::Publisher,
        provider: Some("fixture-provider".to_owned()),
        source_payload_digest: ContentDigest::from_bytes([0x55; 32]),
        segments: vec![
            segment(
                "Small daily cues make useful habits easier to repeat every morning without relying on motivation alone.",
                10_000,
                20_000,
                1,
            ),
            segment(
                "A visible prompt reduces the effort required to remember the intended action when attention is divided.",
                20_000,
                31_000,
                2,
            ),
            segment(
                "Reviewing the same cue after a week reveals whether the behavior has become durable or still needs support.",
                31_000,
                44_000,
                1,
            ),
        ],
    }
}

pub(super) fn evidence_policy() -> EvidenceChunkPolicy {
    EvidenceChunkPolicy {
        version: 1,
        target_tokens: 20,
        overlap_per_mille: 0,
        snap_tolerance_per_mille: 0,
    }
}

pub(super) fn recall_command(
    id: u64,
    query_id: u64,
    text: &str,
    scope: RecallScope,
    limit: u16,
) -> CommandEnvelope {
    CommandEnvelope {
        command_id: CommandId::from_parts(30, id),
        cancellation_id: CancellationId::from_parts(31, id),
        expected_revision: None,
        command: ApplicationCommand::RecallQuery {
            query: RecallQuery {
                query_id: RecallQueryId::from_parts(32, query_id),
                text: text.to_owned(),
                scope,
                limit,
            },
        },
    }
}

pub(super) fn recall_projection(facade: &Pod0Facade, query_id: u64) -> RecallResultProjection {
    let Projection::Recall { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Recall {
                query_id: RecallQueryId::from_parts(32, query_id),
            },
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected recall projection");
    };
    value
}

pub(super) fn recall_request(query_id: u64) -> ProjectionRequest {
    ProjectionRequest {
        scope: ProjectionScope::Recall {
            query_id: RecallQueryId::from_parts(32, query_id),
        },
        offset: 0,
        max_items: 20,
    }
}

pub(super) fn record(
    facade: &Pod0Facade,
    request: &HostRequestEnvelope,
    observation: HostObservation,
) {
    facade.record_host_observation(HostObservationEnvelope {
        request_id: request.request_id,
        cancellation_id: request.cancellation_id,
        observed_request_revision: request.issued_revision,
        sequence_number: 0,
        observed_at: UnixTimestampMilliseconds::new(1_800_000_000_200),
        observation,
    });
}

pub(super) fn run_ready_recall(rerank_failure: bool) -> RecallResultProjection {
    let fixture = RecallFixture::new(true);
    fixture.dispatch(1, 1, "  durable   habit cues ");
    advance_to_rerank(&fixture, 1);
    let rerank = fixture.base.facade.next_host_requests(1).pop().unwrap();
    let HostRequest::RerankRecallCandidates { candidates, .. } = &rerank.request else {
        panic!("expected rerank request");
    };
    let revision = fixture
        .base
        .facade
        .snapshot(recall_request(1))
        .state_revision;
    let observation = if rerank_failure {
        HostObservation::Failed {
            code: HostFailureCode::ProviderUnavailable,
            safe_detail: None,
        }
    } else {
        HostObservation::RecallCandidatesReranked {
            query_id: RecallQueryId::from_parts(32, 1),
            rankings: vec![
                RecallRerankObservation {
                    span_id: candidates[1].span_id,
                    rank: 1,
                },
                RecallRerankObservation {
                    span_id: candidates[0].span_id,
                    rank: 2,
                },
            ],
        }
    };
    record(&fixture.base.facade, &rerank, observation.clone());
    let projection = fixture.projection(1);
    record(&fixture.base.facade, &rerank, observation);
    assert_eq!(
        fixture
            .base
            .facade
            .snapshot(recall_request(1))
            .state_revision,
        StateRevision::new(revision.value + 1)
    );
    projection
}

pub(super) fn advance_to_rerank(fixture: &RecallFixture, query_id: u64) {
    let embed = fixture.base.facade.next_host_requests(1).pop().unwrap();
    let HostRequest::EmbedRecallQuery { text, .. } = &embed.request else {
        panic!("expected embedding request");
    };
    assert!(!text.contains("  "));
    record(
        &fixture.base.facade,
        &embed,
        HostObservation::RecallQueryEmbedded {
            query_id: RecallQueryId::from_parts(32, query_id),
            embedding: RecallEmbeddingVector {
                values: recall_test_embedding(),
            },
        },
    );
    assert_eq!(
        fixture.projection(query_id).stage,
        RecallStage::Running {
            phase: RecallPhase::Reranking
        }
    );
}

pub(super) fn complete_evidence_embedding_requests(facade: &Pod0Facade) {
    loop {
        let Some(request) = facade.next_host_requests(1).pop() else {
            break;
        };
        let HostRequest::EmbedRecallSpans {
            episode_id,
            generation_id,
            spans,
            ..
        } = &request.request
        else {
            panic!("expected evidence embedding request")
        };
        record(
            facade,
            &request,
            HostObservation::RecallSpansEmbedded {
                episode_id: *episode_id,
                generation_id: *generation_id,
                embeddings: spans
                    .iter()
                    .map(|span| RecallSpanEmbeddingObservation {
                        span_id: span.span_id,
                        embedding: RecallEmbeddingVector {
                            values: recall_test_embedding(),
                        },
                    })
                    .collect(),
            },
        );
    }
}

pub(super) fn recall_test_embedding() -> Vec<i32> {
    let mut values = vec![0; pod0_recall_index::RECALL_INDEX_DIMENSIONS];
    values[0] = 1_000_000;
    values
}

fn segment(
    text: &str,
    start_milliseconds: u64,
    end_milliseconds: u64,
    speaker: u64,
) -> TranscriptSegmentInput {
    TranscriptSegmentInput {
        text: text.to_owned(),
        start_milliseconds,
        end_milliseconds,
        speaker_id: Some(SpeakerId::from_parts(0, speaker)),
    }
}
