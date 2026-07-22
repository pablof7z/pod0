import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.Pod0Facade

fun qualifyRecallConfigurationContract(
    facade: Pod0Facade,
    libraryRequest: ProjectionRequest,
) {
    val command = ApplicationCommand.SetRecallConfiguration(
        StateRevision(0UL),
        RecallConfigurationInput("ollama:qwen3-embedding", false),
    )
    check(command.configuration.storedEmbeddingModelId == "ollama:qwen3-embedding")
    facade.dispatch(
        CommandEnvelope(
            CommandId(0UL, 9UL),
            CancellationId(0UL, 10UL),
            null,
            command,
        ),
    )
    val failed = facade.snapshot(libraryRequest).projection
    check(failed is Projection.Library)
    check(failed.value.operations.any { operation ->
        operation.commandId == CommandId(0UL, 9UL) &&
            operation.failure?.code == CoreFailureCode.StorageUnavailable
    })

    val defaultValue = facade.snapshot(
        ProjectionRequest(ProjectionScope.RecallConfiguration, 0u, 1u.toUShort()),
    ).projection
    check(defaultValue is Projection.RecallConfiguration)
    check(defaultValue.value.origin == RecallConfigurationOrigin.Default)
    check(defaultValue.value.embeddingProvider == RecallEmbeddingProvider.OpenRouter)
    check(defaultValue.value.embeddingModel == "openai/text-embedding-3-large")
    check(defaultValue.value.rerankerProvider == null)
}
