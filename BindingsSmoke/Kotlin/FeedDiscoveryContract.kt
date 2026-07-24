import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.Pod0Facade

fun qualifyFeedDiscoveryContract() {
    val occurrenceId = FeedDiscoveryOccurrenceId(1UL, 2UL)
    val episodeId = EpisodeId(3UL, 4UL)
    val podcastId = PodcastId(5UL, 6UL)

    val command = ApplicationCommand.SetNewEpisodeNotificationsEnabled(false)
    check(!command.enabled)
    val settings = NewEpisodeNotificationSettingsProjection(true, StateRevision(7UL))
    check(Projection.NewEpisodeNotificationSettings(settings).value == settings)
    val scope: ProjectionScope = ProjectionScope.NewEpisodeNotificationSettings
    check(scope == ProjectionScope.NewEpisodeNotificationSettings)

    val request = HostRequest.DeliverNewEpisodeNotification(
        occurrenceId,
        episodeId,
        podcastId,
        "Podcast",
        "Episode",
    )
    check(request.occurrenceId == occurrenceId)
    check(request.episodeId == episodeId)
    check(request.podcastId == podcastId)
    check(request.podcastTitle == "Podcast")
    check(request.episodeTitle == "Episode")

    val observation = HostObservation.NewEpisodeNotificationDelivered(
        occurrenceId,
        episodeId,
    )
    check(observation.occurrenceId == occurrenceId)
    check(observation.episodeId == episodeId)
    val wake = CoreWakeReason.FeedDiscoveryNotificationRetry(
        occurrenceId,
        episodeId,
        2u.toUByte(),
    )
    check(wake.attempt == 2u.toUByte())

    val facade = Pod0Facade()
    try {
        val envelope = facade.snapshot(
            ProjectionRequest(
                ProjectionScope.NewEpisodeNotificationSettings,
                0u,
                1u,
            ),
        )
        check(envelope.contractVersion == 46u)
        val projection = envelope.projection
        check(projection is Projection.NewEpisodeNotificationSettings)
        check(projection.value.enabled)
        check(projection.value.revision == StateRevision(0UL))
    } finally {
        facade.destroy()
    }
}
