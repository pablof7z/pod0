use crate::*;

pub(super) fn enable_test_reranking(facade: &Pod0Facade) {
    facade.dispatch(CommandEnvelope {
        command_id: CommandId::from_parts(19, 1),
        cancellation_id: CancellationId::from_parts(19, 2),
        expected_revision: None,
        command: ApplicationCommand::ImportLegacyRecallConfiguration {
            configuration: RecallConfigurationInput {
                stored_embedding_model_id: "openai/text-embedding-3-large".to_owned(),
                reranker_enabled: true,
            },
            source_generation: ContentDigest::from_bytes([0x19; 32]),
        },
    });
}
