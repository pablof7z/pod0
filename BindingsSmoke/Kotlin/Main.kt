import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*
import java.io.File
import java.nio.file.Files

private class RecordingSubscriber : ProjectionSubscriber {
    val revisions = mutableListOf<ULong>()

    override fun receive(projection: ProjectionEnvelope) {
        revisions.add(projection.stateRevision.value)
    }
}

fun main(args: Array<String>) {
    check(args.size == 3)
    val fixture = decodeProperties(File(args[0]).readText())
    check(fixture["fixture_version"] == "1")
    check(fixture["schema_component"] == "kernel")
    check(fixture["stored_version"]?.toUInt() == 2u)
    check(fixture["supported_min"]?.toUInt() == 0u)
    check(fixture["supported_max"]?.toUInt() == 7u)
    check(fixture["access_mode"] == "migration_only")
    check(fixture["migration_state"] == "required")
    check(fixture["target_version"]?.toUInt() == 7u)
    check(fixture["store_id_high"]?.toULong() == 10UL)
    check(fixture["store_id_low"]?.toULong() == 11UL)
    check(fixture["command_id_high"]?.toULong() == 1UL)
    check(fixture["command_id_low"]?.toULong() == 2UL)
    check(fixture["state_revision"]?.toULong() == 42UL)
    check(fixture["operation_stage"] == "failed")
    check(fixture["error_kind"] == "unsupported")
    check(fixture["error_wire_code"]?.toUInt() == 9001u)
    check(fixture["optional_safe_detail"] == "null")

    qualifyListeningDomain(decodeProperties(File(args[1]).readText()))
    qualifyListeningImport(File(args[2]))
    qualifyNativeHostContract()

    val facade = Pod0Facade()
    try {
        val subscriber = RecordingSubscriber()
        val request = ProjectionRequest(ProjectionScope.Library, 0u, 20u.toUShort())
        val handle = facade.subscribe(request, subscriber)
        check(subscriber.revisions == listOf(0UL))

        facade.dispatch(
            uniffi.pod0_application.CommandEnvelope(
                CommandId(0UL, 1UL),
                CancellationId(0UL, 2UL),
                null,
                ApplicationCommand.Unsupported(77u),
            ),
        )
        check(subscriber.revisions == listOf(0UL, 1UL))

        val projection = facade.snapshot(request).projection
        check(facade.snapshot(request).contractVersion == 5u)
        check(projection is Projection.Library)
        val unsupportedOperation = projection.value.operations.single()
        check(unsupportedOperation.commandId == CommandId(0UL, 1UL))
        check(unsupportedOperation.cancellationId == CancellationId(0UL, 2UL))
        check(unsupportedOperation.stage is OperationStage.Failed)
        val unsupportedFailure = unsupportedOperation.failure
        check(unsupportedFailure?.code == CoreFailureCode.Unsupported(77u))
        check(unsupportedFailure.safeDetail == null)

        facade.dispatch(
            uniffi.pod0_application.CommandEnvelope(
                CommandId(0UL, 3UL),
                CancellationId(0UL, 4UL),
                null,
                ApplicationCommand.SubscribeToFeed("https://example.test/feed"),
            ),
        )
        facade.dispatch(
            uniffi.pod0_application.CommandEnvelope(
                CommandId(0UL, 5UL),
                CancellationId(0UL, 6UL),
                null,
                ApplicationCommand.CancelOperation(CancellationId(0UL, 4UL)),
            ),
        )
        check(facade.nextHostRequests(64u.toUShort()).isEmpty())
        val cancelledProjection = facade.snapshot(request).projection
        check(cancelledProjection is Projection.Library)
        check(cancelledProjection.value.operations.any { operation ->
            operation.commandId == CommandId(0UL, 3UL) &&
                operation.stage is OperationStage.Cancelled &&
                operation.failure?.code is CoreFailureCode.Cancelled
        })

        facade.unsubscribe(handle)
        facade.dispatch(
            uniffi.pod0_application.CommandEnvelope(
                CommandId(0UL, 7UL),
                CancellationId(0UL, 8UL),
                null,
                ApplicationCommand.Unsupported(78u),
            ),
        )
        check(subscriber.revisions == listOf(0UL, 1UL, 2UL, 3UL))
    } finally {
        facade.destroy()
    }
}

private fun qualifyListeningImport(source: File) {
    val root = Files.createTempDirectory("pod0-listening-import").toFile()
    try {
        val plan = inspectLegacyListeningSource(source.absolutePath)
        check(plan.sourceKind == LegacyListeningSourceKind.LEGACY_JSON)
        check(plan.podcastCount == 1u && plan.subscriptionCount == 1u && plan.episodeCount == 1u)
        val report = stageLegacyListeningImport(
            source.absolutePath,
            File(root, "source.backup.json").absolutePath,
            File(root, "core.sqlite").absolutePath,
            File(root, "core.backup.sqlite").absolutePath,
            plan,
            CommandId(0UL, 1UL),
            CommandId(0UL, 2UL),
            1_721_322_000_000L,
        )
        check(report.staged && !report.reusedExisting)
        val imported = readStagedLegacyListeningImport(
            File(root, "core.sqlite").absolutePath,
            CommandId(0UL, 1UL),
        ).snapshot
        check(imported.podcasts.single().title == "Legacy Kotlin fixture")
        check(imported.episodes.single().listening.resumePositionMilliseconds == 32_250UL)
        check(imported.episodes.single().listening.completion == CompletionStatus.Completed(CompletionCause.LegacyPlayedFlag))
        check(imported.episodes.single().isStarred)
        check(imported.playback.activeEpisodeId == imported.episodes.single().episodeId)
    } finally {
        root.deleteRecursively()
    }
}

private fun qualifyListeningDomain(fixture: Map<String, String>) {
    check(fixture["fixture_version"] == "1")
    check(fixture["unknown_future_field"] == "ignored-by-v1-readers")
    check(fixture["completion_percentage_threshold"] == "none")
    val podcastId = PodcastId(
        fixture.getValue("podcast_id_high").toULong(),
        fixture.getValue("podcast_id_low").toULong(),
    )
    val incomingPodcastId = PodcastId(
        fixture.getValue("incoming_podcast_id_high").toULong(),
        fixture.getValue("incoming_podcast_id_low").toULong(),
    )
    val episodeId = EpisodeId(
        fixture.getValue("episode_id_high").toULong(),
        fixture.getValue("episode_id_low").toULong(),
    )
    val feed = makeFeedIdentityV1(fixture.getValue("feed_source_url"))
    check(feed.comparisonKey == fixture["feed_comparison_key"])
    check(
        resolvePodcastIdentityV1(
            incomingPodcastId,
            fixture.getValue("feed_source_url"),
            listOf(PodcastIdentityRecord(podcastId, feed)),
        ) == PodcastIdentityResolution.PreserveExisting(podcastId),
    )
    check(resolveLegacyParentId(podcastId, incomingPodcastId) == podcastId)
    check(resolveLegacyParentId(null, podcastId) == podcastId)
    val incomingEpisodeId = EpisodeId(
        fixture.getValue("incoming_episode_id_high").toULong(),
        fixture.getValue("incoming_episode_id_low").toULong(),
    )
    check(
        resolveEpisodeIdentityV1(
            incomingEpisodeId,
            podcastId,
            fixture.getValue("episode_guid"),
            listOf(EpisodeIdentityRecord(episodeId, podcastId, fixture.getValue("episode_guid"))),
        ) == EpisodeIdentityResolution.PreserveExisting(episodeId),
    )

    fun artifact(version: String, key: String) = ArtifactReference(
        fixture.getValue(version).toUInt(),
        fixture.getValue(key),
    )
    val queue = listOf(
        QueueEntry(
            QueueEntryId(
                fixture.getValue("queue_whole_id_high").toULong(),
                fixture.getValue("queue_whole_id_low").toULong(),
            ),
            episodeId,
            null,
            null,
        ),
        QueueEntry(
            QueueEntryId(
                fixture.getValue("queue_segment_id_high").toULong(),
                fixture.getValue("queue_segment_id_low").toULong(),
            ),
            episodeId,
            PlaybackSegment(
                fixture.getValue("queue_segment_start_ms").toULong(),
                fixture.getValue("queue_segment_end_ms").toULong(),
            ),
            fixture["queue_segment_label"],
        ),
    )
    val snapshot = ListeningDomainSnapshot(
        podcasts = listOf(
            PodcastRecord(
                podcastId,
                PodcastKind.Rss,
                feed,
                fixture.getValue("podcast_title"),
                fixture.getValue("podcast_author"),
                fixture["podcast_image_url"],
                fixture.getValue("podcast_description"),
                fixture["podcast_language"],
                fixture.getValue("podcast_categories").split(","),
                UnixTimestampMilliseconds(fixture.getValue("podcast_discovered_at_ms").toLong()),
                fixture.getValue("podcast_title_is_placeholder").toBooleanStrict(),
                UnixTimestampMilliseconds(fixture.getValue("podcast_last_refreshed_at_ms").toLong()),
                fixture["podcast_etag"],
                fixture["podcast_last_modified"],
            ),
        ),
        subscriptions = listOf(
            PodcastSubscriptionRecord(
                podcastId,
                UnixTimestampMilliseconds(fixture.getValue("subscription_subscribed_at_ms").toLong()),
                AutoDownloadPolicy(
                    AutoDownloadMode.Latest(fixture.getValue("auto_download_latest_count").toUShort()),
                    fixture.getValue("auto_download_wifi_only").toBooleanStrict(),
                ),
                fixture.getValue("notifications_enabled").toBooleanStrict(),
                PlaybackRatePermille(fixture.getValue("default_playback_rate_permille").toUShort()),
            ),
        ),
        episodes = listOf(
            EpisodeRecord(
                episodeId,
                podcastId,
                fixture.getValue("episode_guid"),
                fixture.getValue("episode_title"),
                fixture.getValue("episode_description"),
                UnixTimestampMilliseconds(fixture.getValue("episode_published_at_ms").toLong()),
                fixture.getValue("episode_duration_ms").toULong(),
                fixture.getValue("episode_enclosure_url"),
                fixture["episode_enclosure_mime"],
                fixture["episode_image_url"],
                EpisodeFeedMetadata(null, null, emptyList(), emptyList()),
                EpisodeListeningState(
                    fixture.getValue("episode_resume_position_ms").toULong(),
                    CompletionStatus.InProgress,
                ),
                fixture.getValue("episode_is_starred").toBooleanStrict(),
                DownloadArtifactStatus.Available(
                    artifact("download_schema_version", "download_opaque_key"),
                    fixture.getValue("download_byte_count").toULong(),
                ),
                TranscriptArtifactStatus.Available(
                    artifact("transcript_schema_version", "transcript_opaque_key"),
                    TranscriptSource.Publisher,
                ),
            ),
        ),
        playback = ListeningPlaybackPolicy(
            activeEpisodeId = episodeId,
            activeSegment = null,
            activeLabel = null,
            queue = queue,
            rate = PlaybackRatePermille(
                fixture.getValue("playback_rate_permille").toUShort()
            ),
            sleepMode = PlaybackSleepMode.Duration(
                fixture.getValue("sleep_duration_ms").toULong()
            ),
            autoMarkPlayedAtNaturalEnd = fixture
                .getValue("auto_mark_played_at_natural_end")
                .toBooleanStrict(),
            autoPlayNext = fixture.getValue("auto_play_next").toBooleanStrict(),
            revision = StateRevision(fixture.getValue("state_revision").toULong()),
        ),
    )
    check(validateListeningSnapshot(snapshot) == snapshot)
    check(snapshot.playback.queue.map { it.episodeId } == listOf(episodeId, episodeId))
    check(snapshot.playback.queue[0].queueEntryId != snapshot.playback.queue[1].queueEntryId)
}

private fun decodeProperties(text: String): Map<String, String> =
    text.lineSequence()
        .filter { line -> line.isNotEmpty() && !line.startsWith("#") }
        .associate { line ->
            val separator = line.indexOf('=')
            check(separator > 0)
            line.substring(0, separator) to line.substring(separator + 1)
        }
