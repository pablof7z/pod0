use crate::runtime_recall_test_support::{
    RecallFixture, complete_evidence_embedding_requests, record,
};
use crate::*;

#[test]
fn configuration_change_reindexes_with_typed_provider_and_survives_restart() {
    let fixture = RecallFixture::new(true);
    let before = configuration(&fixture.base.facade);
    assert_eq!(before.origin, RecallConfigurationOrigin::LegacySwift);
    assert!(before.reranker_enabled);

    let command_id = CommandId::from_parts(81, 1);
    fixture.base.facade.dispatch(CommandEnvelope {
        command_id,
        cancellation_id: CancellationId::from_parts(81, 2),
        expected_revision: None,
        command: ApplicationCommand::SetRecallConfiguration {
            expected_configuration_revision: before.revision,
            configuration: RecallConfigurationInput {
                stored_embedding_model_id: "ollama:qwen3-embedding".to_owned(),
                reranker_enabled: false,
            },
        },
    });

    let request = fixture.base.facade.next_host_requests(1).pop().unwrap();
    assert!(matches!(
        &request.request,
        HostRequest::EmbedRecallSpans {
            provider: RecallEmbeddingProvider::Ollama,
            model,
            maximum_dimensions: 1_024,
            ..
        } if model == "qwen3-embedding"
    ));
    fixture
        .base
        .facade
        .record_host_observation(HostObservationEnvelope {
            request_id: request.request_id,
            cancellation_id: request.cancellation_id,
            observed_request_revision: request.issued_revision,
            sequence_number: 0,
            observed_at: UnixTimestampMilliseconds::new(1_800_000_000_300),
            observation: match &request.request {
                HostRequest::EmbedRecallSpans {
                    episode_id,
                    generation_id,
                    spans,
                    ..
                } => HostObservation::RecallSpansEmbedded {
                    episode_id: *episode_id,
                    generation_id: *generation_id,
                    embeddings: spans
                        .iter()
                        .map(|span| RecallSpanEmbeddingObservation {
                            span_id: span.span_id,
                            embedding: RecallEmbeddingVector {
                                values: {
                                    let mut values = vec![0; 1_024];
                                    values[0] = 1_000_000;
                                    values
                                },
                            },
                        })
                        .collect(),
                },
                _ => unreachable!(),
            },
        });
    complete_evidence_embedding_requests(&fixture.base.facade);

    let after = configuration(&fixture.base.facade);
    assert_eq!(after.origin, RecallConfigurationOrigin::User);
    assert_eq!(after.embedding_provider, RecallEmbeddingProvider::Ollama);
    assert_eq!(after.embedding_model, "qwen3-embedding");
    assert!(!after.reranker_enabled);
    let operation = operation(&fixture.base.facade, command_id);
    assert_eq!(operation.stage, OperationStage::Succeeded);
    assert_eq!(
        operation.result,
        Some(OperationResult::RecallConfigurationUpdated {
            revision: after.revision,
            reindexed_episode_count: 1,
        })
    );

    let reopened = Pod0Facade::open(fixture.base.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(configuration(&reopened), after);

    fixture.dispatch(83, 83, "habit cues");
    let query_request = fixture.base.facade.next_host_requests(1).pop().unwrap();
    assert!(matches!(
        &query_request.request,
        HostRequest::EmbedRecallQuery {
            provider: RecallEmbeddingProvider::Ollama,
            model,
            ..
        } if model == "qwen3-embedding"
    ));
    record(
        &fixture.base.facade,
        &query_request,
        HostObservation::RecallQueryEmbedded {
            query_id: RecallQueryId::from_parts(32, 83),
            embedding: RecallEmbeddingVector {
                values: {
                    let mut values = vec![0; 1_024];
                    values[0] = 1_000_000;
                    values
                },
            },
        },
    );
    assert_eq!(fixture.projection(83).stage, RecallStage::Ready);
    assert!(fixture.base.facade.next_host_requests(1).is_empty());
}

#[test]
fn invalid_or_stale_configuration_never_replaces_authoritative_state() {
    let fixture = RecallFixture::new(false);
    let before = configuration(&fixture.base.facade);
    let invalid_id = CommandId::from_parts(82, 1);
    fixture.base.facade.dispatch(CommandEnvelope {
        command_id: invalid_id,
        cancellation_id: CancellationId::from_parts(82, 2),
        expected_revision: None,
        command: ApplicationCommand::SetRecallConfiguration {
            expected_configuration_revision: before.revision,
            configuration: RecallConfigurationInput {
                stored_embedding_model_id: " \n ".to_owned(),
                reranker_enabled: false,
            },
        },
    });
    assert_eq!(
        operation(&fixture.base.facade, invalid_id).stage,
        OperationStage::Failed
    );
    assert_eq!(configuration(&fixture.base.facade), before);

    let stale_id = CommandId::from_parts(82, 3);
    fixture.base.facade.dispatch(CommandEnvelope {
        command_id: stale_id,
        cancellation_id: CancellationId::from_parts(82, 4),
        expected_revision: None,
        command: ApplicationCommand::SetRecallConfiguration {
            expected_configuration_revision: StateRevision::INITIAL,
            configuration: RecallConfigurationInput {
                stored_embedding_model_id: "ollama:qwen3-embedding".to_owned(),
                reranker_enabled: false,
            },
        },
    });
    assert_eq!(
        operation(&fixture.base.facade, stale_id).stage,
        OperationStage::Failed
    );
    assert_eq!(configuration(&fixture.base.facade), before);
}

#[test]
fn first_user_change_persists_when_no_legacy_configuration_exists() {
    let base = crate::runtime_playback_test_support::PlaybackFixture::new();
    let before = configuration(&base.facade);
    assert_eq!(before.origin, RecallConfigurationOrigin::Default);
    assert_eq!(before.revision, StateRevision::INITIAL);

    let command_id = CommandId::from_parts(84, 1);
    base.facade.dispatch(CommandEnvelope {
        command_id,
        cancellation_id: CancellationId::from_parts(84, 2),
        expected_revision: None,
        command: ApplicationCommand::SetRecallConfiguration {
            expected_configuration_revision: before.revision,
            configuration: RecallConfigurationInput {
                stored_embedding_model_id: "ollama:qwen3-embedding".to_owned(),
                reranker_enabled: false,
            },
        },
    });
    let after = configuration(&base.facade);
    assert_eq!(after.origin, RecallConfigurationOrigin::User);
    assert_eq!(
        operation(&base.facade, command_id).stage,
        OperationStage::Succeeded
    );

    let reopened = Pod0Facade::open(base.target.to_string_lossy().into_owned()).unwrap();
    assert_eq!(configuration(&reopened), after);
}

fn configuration(facade: &Pod0Facade) -> RecallConfiguration {
    let Projection::RecallConfiguration { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::RecallConfiguration,
            offset: 0,
            max_items: 1,
        })
        .projection
    else {
        panic!("expected recall configuration")
    };
    value
}

fn operation(facade: &Pod0Facade, command_id: CommandId) -> OperationProjection {
    let Projection::Library { value } = facade
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 20,
        })
        .projection
    else {
        panic!("expected library")
    };
    value
        .operations
        .into_iter()
        .find(|value| value.command_id == command_id)
        .unwrap()
}
