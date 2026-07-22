import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.Pod0Facade

fun qualifyDownloadContract() {
    val episodeId = EpisodeId(7UL, 8UL)
    val intentId = DownloadIntentId(9UL, 10UL)
    val attemptId = DownloadAttemptId(11UL, 12UL)
    val requestCommand = ApplicationCommand.RequestEpisodeDownload(
        episodeId,
        DownloadIntentOrigin.User,
    )
    check(requestCommand.episodeId == episodeId)
    check(requestCommand.origin == DownloadIntentOrigin.User)
    val automaticCandidates = ApplicationCommand.ReportAutomaticDownloadCandidates(
        PodcastId(5UL, 6UL),
        listOf(episodeId),
    )
    check(automaticCandidates.episodeIds == listOf(episodeId))

    val cancelCommand = ApplicationCommand.CancelEpisodeDownload(
        episodeId,
        StateRevision(13UL),
    )
    val removeCommand = ApplicationCommand.RemoveEpisodeDownload(
        episodeId,
        StateRevision(14UL),
    )
    check(cancelCommand.expectedWorkflowRevision.value == 13UL)
    check(removeCommand.expectedWorkflowRevision.value == 14UL)

    val environment = ApplicationCommand.ObserveDownloadEnvironment(
        DownloadEnvironmentObservation(
            DownloadNetworkState.Wifi,
            512UL * 1_024UL * 1_024UL,
        ),
    )
    check(environment.observation.network == DownloadNetworkState.Wifi)

    val start = HostRequest.StartEpisodeDownload(
        episodeId,
        intentId,
        attemptId,
        "a".repeat(64),
        "https://example.test/audio.mp3",
        null,
    )
    check(start.attemptId == attemptId)
    check(start.enclosureUrl == "https://example.test/audio.mp3")

    val cancel = HostRequest.CancelEpisodeDownload(
        episodeId,
        intentId,
        attemptId,
        "task-1",
    )
    val remove = HostRequest.RemoveEpisodeDownloadArtifact(
        episodeId,
        "downloads/episode-8.mp3",
    )
    check(cancel.externalTaskKey == "task-1")
    check(remove.artifactKey == "downloads/episode-8.mp3")

    val accepted = HostObservation.DownloadAccepted(
        episodeId,
        intentId,
        attemptId,
        "task-1",
        null,
    )
    val staged = HostObservation.DownloadStaged(
        episodeId,
        intentId,
        attemptId,
        "/tmp/download-12",
        4_096UL,
    )
    check(accepted.externalTaskKey == "task-1")
    check(staged.byteCount == 4_096UL)

    val cancelled = HostObservation.DownloadCancelled(episodeId, intentId, attemptId)
    val removed = HostObservation.DownloadArtifactRemoved(
        episodeId,
        "downloads/episode-8.mp3",
    )
    check(cancelled.attemptId == attemptId)
    check(removed.artifactKey == "downloads/episode-8.mp3")

    val facade = Pod0Facade()
    try {
        facade.dispatch(
            CommandEnvelope(
                CommandId(0UL, 201UL),
                CancellationId(0UL, 202UL),
                null,
                requestCommand,
            ),
        )
        val projection = facade.snapshot(
            ProjectionRequest(
                ProjectionScope.Downloads(null),
                0u,
                20u.toUShort(),
            ),
        )
        check(projection.contractVersion == 34u)
        val projected = projection.projection
        check(projected is Projection.Downloads)
        val downloads = projected.value
        check(downloads.workflows.isEmpty())
        check(downloads.failure?.code == CoreFailureCode.StorageUnavailable)
    } finally {
        facade.destroy()
    }
}

fun decodeProperties(text: String): Map<String, String> =
    text.lineSequence()
        .filter { line -> line.isNotEmpty() && !line.startsWith("#") }
        .associate { line ->
            val separator = line.indexOf('=')
            check(separator > 0)
            line.substring(0, separator) to line.substring(separator + 1)
        }
