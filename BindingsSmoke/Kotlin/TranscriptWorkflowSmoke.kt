import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*

fun qualifyTranscriptWorkflowContract() {
    val episodeId = EpisodeId(0UL, 501UL)
    val context = TranscriptCapabilityContext(
        episodeId = episodeId,
        podcastId = PodcastId(0UL, 502UL),
        sourceRevision = "audio-v1",
    )
    val plan = planTranscriptWorkflow(
        TranscriptWorkflowPlanInput(
            episodeId = episodeId,
            sourceRevision = "audio-v1",
            committedTranscript = null,
            selectedEvidenceInputVersion = null,
            origin = TranscriptWorkflowOrigin.User,
            configuredProvider = TranscriptProvider.AssemblyAi,
            configuredModel = "universal-3-pro",
            remoteAudioUrl = "https://example.test/audio.mp3",
            localAudioUrl = null,
            publisherTranscriptUrl = "https://example.test/transcript.vtt",
            publisherMimeHint = "text/vtt",
            autoPublisherEnabled = true,
            autoProviderEnabled = true,
            credentialAvailable = true,
            embeddingSpaceId = "embedding-space-v1",
        ),
    )
    check(plan.generation is TranscriptGenerationDecision.Ensure)
    val workflowRequest = checkNotNull(plan.request)
    check(!workflowRequest.publisherFirst)

    val accepted = validateTranscriptCapabilityRequest(
        TranscriptCapabilityRequest.FetchPublisher(
            context = context,
            sourceUrl = "https://example.test/transcript.vtt",
            mimeHint = "text/vtt",
            maximumResponseBytes = 1_024UL,
        ),
    )
    check(accepted is TranscriptCapabilityValidation.Accepted)

    val rejected = validateTranscriptCapabilityRequest(
        TranscriptCapabilityRequest.SubmitProvider(
            context = context,
            attemptId = TranscriptAttemptId(0UL, 1UL),
            submissionFenceId = TranscriptSubmissionFenceId(0UL, 2UL),
            provider = TranscriptProvider.AppleSpeech,
            model = "apple-speech-v1",
            audioUrl = "file:///tmp/audio.m4a",
            maximumResponseBytes = 1_024UL,
        ),
    )
    check(rejected is TranscriptCapabilityValidation.Rejected)

    check(
        validateTranscriptCapabilityObservation(
            TranscriptCapabilityObservation.ProviderAccepted(
                externalOperationId = "provider-operation-1",
                providerStatus = "queued",
            ),
        ) is TranscriptCapabilityValidation.Accepted,
    )
    check(
        validateTranscriptCapabilityObservation(
            TranscriptCapabilityObservation.ProviderPending(
                providerStatus = "processing",
                retryAfterMilliseconds = 5_000UL,
            ),
        ) is TranscriptCapabilityValidation.Accepted,
    )
    check(
        validateTranscriptCapabilityObservation(
            TranscriptCapabilityObservation.Cancelled,
        ) is TranscriptCapabilityValidation.Accepted,
    )
}
