import uniffi.pod0_application.*
import uniffi.pod0_domain.*

fun qualifyNativeHostContract() {
    val episodeId = EpisodeId(4UL, 2UL)
    val requests = listOf(
        HostRequest.FetchFeed("https://feeds.example.test/show.xml", "\"v1\"", null, 8_388_608UL),
        HostRequest.LoadMedia(episodeId, "https://cdn.example.test/episode.mp3", 12_500UL),
        HostRequest.Play(episodeId, PlaybackTransitionCue.Immediate),
        HostRequest.Pause(episodeId),
        HostRequest.Seek(episodeId, 25_000UL),
        HostRequest.SetRate(episodeId, PlaybackRatePermille(1_500u.toUShort())),
        HostRequest.ArmNativeTimer(episodeId, NativeTimerMode.Duration(60_000UL)),
        HostRequest.CancelNativeTimer(episodeId),
        HostRequest.ObservePlayback(episodeId, 1_000u),
    )
    check(requests.size == 9)
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
}
