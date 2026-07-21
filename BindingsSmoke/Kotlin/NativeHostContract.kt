import uniffi.pod0_application.*
import uniffi.pod0_domain.*

fun qualifyNativeHostContract() {
    val episodeId = EpisodeId(4UL, 2UL)
    val chapterContext = ChapterPlaybackContext(
        episodeId,
        ChapterArtifactId(5UL, 6UL),
        StateRevision(7UL),
        ChapterPlaybackSessionId(8UL, 9UL),
        1u,
    )
    val requests = listOf(
        HostRequest.FetchFeed("https://feeds.example.test/show.xml", "\"v1\"", null, 8_388_608UL),
        HostRequest.LoadMedia(episodeId, "https://cdn.example.test/episode.mp3", 12_500UL),
        HostRequest.Play(episodeId, PlaybackTransitionCue.Immediate),
        HostRequest.Pause(episodeId),
        HostRequest.Seek(
            episodeId,
            25_000UL,
            PlaybackSeekReason.NextChapter,
            chapterContext,
        ),
        HostRequest.SetRate(episodeId, PlaybackRatePermille(1_500u.toUShort())),
        HostRequest.ArmNativeTimer(episodeId, NativeTimerMode.Duration(60_000UL)),
        HostRequest.CancelNativeTimer(episodeId),
        HostRequest.ObservePlayback(episodeId, 1_000u),
    )
    check(requests.size == 9)
    val chapterAction = PlaybackCommand.NextChapter(chapterContext, 12_500UL)
    check(chapterAction.context == chapterContext)
    check((requests[4] as HostRequest.Seek).chapterContext == chapterContext)
    val request = HostRequestEnvelope(
        HostRequestId(0UL, 7UL),
        CommandId(0UL, 8UL),
        CancellationId(0UL, 9UL),
        StateRevision(10UL),
        UnixTimestampMilliseconds(1_721_322_030_000L),
        requests.first(),
    )
    val playback = PlaybackLifecycleObservation(
        episodeId,
        PlaybackHostState.Playing,
        12_500UL,
        600_000UL,
        PlaybackAudioRoute.Bluetooth,
        PlaybackInterruption.None,
        false,
    )
    val observation = HostObservationEnvelope(
        request.requestId,
        request.cancellationId,
        request.issuedRevision,
        1UL,
        UnixTimestampMilliseconds(1_721_322_000_000L),
        HostObservation.PlaybackObserved(playback),
    )
    check(observation.observedRequestRevision == request.issuedRevision)
    check((observation.observation as HostObservation.PlaybackObserved).value == playback)

    val fence = ChapterModelSubmissionFenceId(11UL, 12UL)
    val execution = ChapterModelExecutionRequest(
        "openrouter",
        "model-a",
        "Return a JSON object.",
        "Transcript evidence",
        ChapterModelResponseFormat.JsonObject,
        65_536UL,
    )
    val execute = HostRequest.ExecuteChapterModel(episodeId, 2UL, fence, execution)
    val recover = HostRequest.RecoverChapterModelOperation(
        episodeId,
        2UL,
        fence,
        "openrouter",
        "model-a",
        "provider-operation-1",
        "running",
        65_536UL,
    )
    check(execute.execution.systemPrompt == "Return a JSON object.")
    check(recover.providerOperationId == "provider-operation-1")
    val providerUpdate = HostObservation.ChapterModelProviderAccepted(
        episodeId,
        2UL,
        fence,
        ChapterModelProviderUpdate("provider-operation-1", "running"),
    )
    val completion = HostObservation.ChapterModelCompleted(
        episodeId,
        2UL,
        fence,
        ChapterModelCompletionObservation(
            "{\"chapters\":[]}",
            "openrouter",
            "model-a:canonical",
            10UL,
            4UL,
            0UL,
            0UL,
            2UL,
            "provider-operation-1",
            "completed",
            UnixTimestampMilliseconds(1_721_322_000_001L),
        ),
    )
    check(providerUpdate.update.providerStatus == "running")
    check(completion.completion.model == "model-a:canonical")
    val failed = HostObservation.ChapterModelFailed(
        episodeId,
        2UL,
        fence,
        ChapterModelHostFailureCode.HttpResponse(429u.toUShort()),
        "rate limited",
        30_000UL,
    )
    check(failed.retryAfterMilliseconds == 30_000UL)
    val wakeReason = CoreWakeReason.ModelChapterRetry(episodeId, 2UL, fence)
    val wake = HostRequest.ScheduleCoreWake(
        UnixTimestampMilliseconds(1_721_322_030_000L),
        wakeReason,
    )
    val reached = HostObservation.CoreWakeReached(wakeReason)
    check(wake.reason == reached.reason)
    val durableReceipt = HostObservationReceipt.Persisted(request.requestId, true)
    check(durableReceipt.terminal)
}
