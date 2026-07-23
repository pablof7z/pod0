import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.Pod0Facade

fun qualifyScheduledAgentContract() {
    val digest = ContentDigest(1UL, 2UL, 3UL, 4UL)
    val task = ScheduledTaskInput(
        ScheduledTaskId(5UL, 6UL),
        "Daily briefing",
        "Prepare a daily briefing",
        "openrouter:test/model",
        86_400_000UL,
        UnixTimestampMilliseconds(1_000L),
    )
    val command = ApplicationCommand.EnsureScheduledTask(task)
    check(command.task == task)

    val occurrenceId = ScheduledOccurrenceId(7UL, 8UL)
    val attemptId = ScheduledAttemptId(9UL, 10UL)
    val execution = ScheduledAgentExecutionRequest(
        occurrenceId,
        attemptId,
        digest,
        task.prompt,
        task.modelReference,
        listOf(ScheduledAgentContextMessage(ScheduledAgentContextRole.User, "Use saved evidence")),
        16_384UL,
    )
    val request = HostRequest.ExecuteScheduledAgentTurn(execution)
    check(request.execution == execution)

    val completed = ScheduledAgentExecutionObservation.Completed(
        occurrenceId,
        attemptId,
        GeneratedArtifactId(11UL, 12UL),
        digest,
        "Briefing ready",
    )
    val observation = HostObservation.ScheduledAgentExecutionObserved(completed)
    check(observation.observation == completed)

    val facade = Pod0Facade()
    try {
        val envelope = facade.snapshot(
            ProjectionRequest(
                ProjectionScope.ScheduledAgent(null),
                0u,
                20u.toUShort(),
            ),
        )
        check(envelope.contractVersion == 40u)
        val projection = envelope.projection
        check(projection is Projection.ScheduledAgent)
        check(projection.value.tasks.isEmpty())
        check(projection.value.workflows.isEmpty())
        check(projection.value.failure?.code == CoreFailureCode.StorageUnavailable)
    } finally {
        facade.destroy()
    }
}
