import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*

fun qualifyModelChapterRuntime(facade: Pod0Facade, episodeId: EpisodeId) {
    val staged = facade.stageLegacyModelChapterCutover(
        1UL,
        "ollama:llama3.2",
        emptyList(),
    )
    check(staged.stage == LegacyModelChapterCutoverStage.STAGED)
    val authoritative = facade.commitLegacyModelChapterCutover(1UL)
    check(authoritative.stage == LegacyModelChapterCutoverStage.AUTHORITATIVE)
    facade.dispatch(CommandEnvelope(
        CommandId(0UL, 45UL),
        CancellationId(0UL, 46UL),
        null,
        ApplicationCommand.EnsureModelChapters(episodeId, "ollama:llama3.2"),
    ))
    val request = facade.nextHostRequests(1u.toUShort()).single()
    val execute = request.request
    check(execute is HostRequest.ExecuteChapterModel)
    check(execute.execution.provider == "ollama")
    check(execute.execution.model == "llama3.2")
    val receipt = facade.recordHostObservation(HostObservationEnvelope(
        request.requestId,
        request.cancellationId,
        request.issuedRevision,
        1UL,
        UnixTimestampMilliseconds(1_800_000_100_200L),
        HostObservation.ChapterModelCompleted(
            execute.episodeId,
            execute.generation,
            execute.submissionFenceId,
            ChapterModelCompletionObservation(
                """{"chapters":[{"start":0,"title":"Opening"},{"start":30,"title":"Context"},{"start":60,"title":"Deep dive"},{"start":90,"title":"Close"}],"ads":[]}""",
                "ollama",
                "llama3.2:latest",
                100UL,
                50UL,
                0UL,
                0UL,
                null,
                null,
                "completed",
                null,
            ),
        ),
    ))
    check(receipt == HostObservationReceipt.Persisted(request.requestId, true))
    val projection = facade.snapshot(ProjectionRequest(
        ProjectionScope.ChapterWorkflows(episodeId),
        0u,
        20u.toUShort(),
    )).projection
    check(projection is Projection.ChapterWorkflows)
    val workflow = projection.value.model.single()
    check(workflow.stage is ModelChapterWorkflowStage.Succeeded)
    check(workflow.selectedArtifactId != null)
}
