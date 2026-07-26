import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*

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
    val legacyCandidate = LegacyFeedDiscoveryCandidateInput(
        CommandId(9UL, 10UL),
        podcastId,
        episodeId,
        LegacyFeedDiscoveryEffectKindInput.NOTIFICATION,
        LegacyFeedDiscoveryDispositionInput.Ambiguous(2u.toUByte()),
        UnixTimestampMilliseconds(1_800_000_000_000L),
        UnixTimestampMilliseconds(1_800_086_400_000L),
        UnixTimestampMilliseconds(1_799_999_000_000L),
        "a".repeat(64),
    )
    check(legacyCandidate.disposition is LegacyFeedDiscoveryDispositionInput.Ambiguous)

    val facade = Pod0Facade()
    try {
        val unavailableCutover = facade.feedDiscoveryCutover()
        check(unavailableCutover.stage == LegacyFeedDiscoveryCutoverStage.BLOCKED)
        check(
            unavailableCutover.failure?.code ==
                LegacyFeedDiscoveryCutoverFailureCode.STORAGE_UNAVAILABLE,
        )
        val envelope = facade.snapshot(
            ProjectionRequest(
                ProjectionScope.NewEpisodeNotificationSettings,
                0u,
                1u,
            ),
        )
        check(envelope.contractVersion == 50u)
        val projection = envelope.projection
        check(projection is Projection.NewEpisodeNotificationSettings)
        check(projection.value.enabled)
        check(projection.value.revision == StateRevision(0UL))
    } finally {
        facade.destroy()
    }
}
