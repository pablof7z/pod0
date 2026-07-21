import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*
import java.security.MessageDigest

fun qualifyChapterContract(fixture: Map<String, String>) {
    fun number(key: String) = fixture.getValue(key).toULong()
    fun id(prefix: String) = number("${prefix}_high") to number("${prefix}_low")
    fun optional(key: String) = fixture.getValue(key).takeUnless { it == "none" }
    fun digest(prefix: String) = ContentDigest(
        number("${prefix}_word_0"),
        number("${prefix}_word_1"),
        number("${prefix}_word_2"),
        number("${prefix}_word_3"),
    )
    val chapters = (0 until fixture.getValue("chapter_count").toInt()).map { index ->
        val prefix = "chapter_$index"
        val sourceEpisodeId = if (fixture.getValue("${prefix}_source_episode") == "none") {
            null
        } else {
            val sourceId = id("${prefix}_source_episode_id")
            EpisodeId(sourceId.first, sourceId.second)
        }
        ChapterInput(
            number("${prefix}_start_milliseconds"),
            optional("${prefix}_end_milliseconds")?.toULong(),
            fixture.getValue("${prefix}_title"),
            optional("${prefix}_summary"),
            optional("${prefix}_image_url"),
            optional("${prefix}_link_url"),
            fixture.getValue("${prefix}_include_in_toc").toBooleanStrict(),
            sourceEpisodeId,
        )
    }
    val adSpans = (0 until fixture.getValue("ad_span_count").toInt()).map { index ->
        val prefix = "ad_span_$index"
        check(fixture.getValue("${prefix}_kind") == "midroll")
        AdSpanInput(
            number("${prefix}_start_milliseconds"),
            number("${prefix}_end_milliseconds"),
            ChapterAdKind.Midroll,
        )
    }
    val commandId = id("command_id")
    val episodeId = id("episode_id")
    val podcastId = id("podcast_id")
    val transcriptVersionId = id("transcript_version_id")
    check(fixture["source"] == "publisher_enriched")
    check(fixture["ad_span_evaluation"] == "evaluated")
    val request = ChapterContractRequest(
        CommandId(commandId.first, commandId.second),
        StateRevision(number("expected_selection_revision")),
        ChapterArtifactInput(
            EpisodeId(episodeId.first, episodeId.second),
            PodcastId(podcastId.first, podcastId.second),
            fixture.getValue("source_revision"),
            ChapterArtifactProvenance(
                ChapterArtifactSource.PublisherEnriched,
                fixture["provider"],
                fixture["model"],
                fixture.getValue("policy_version").toUInt(),
                digest("source_payload_digest"),
                TranscriptVersionId(transcriptVersionId.first, transcriptVersionId.second),
                digest("transcript_content_digest"),
                null,
            ),
            UnixTimestampMilliseconds(fixture.getValue("generated_at_milliseconds").toLong()),
            number("duration_milliseconds"),
            chapters,
            AdSpanEvaluation.Evaluated,
            adSpans,
        ),
    )

    check(fixture["fixture_version"] == "1")
    check(fixture["contract_version"]?.toUInt() == 24u)
    check(fixture["unknown_future_field"] == "ignored-by-v1-readers")
    val qualified = projectChapterContract(
        request,
        ChapterProjectionScope.Chapters,
        0u,
        1u.toUShort(),
    )
    check(qualified is ChapterContractProjection.Qualified)
    val receipt = qualified.receipt
    val projection = qualified.artifact
    val artifactId = id("expected_artifact_id")
    check(receipt.artifactId == ChapterArtifactId(artifactId.first, artifactId.second))
    check(receipt.contentDigest == digest("expected_content_digest"))
    check(receipt.integrityDigest == digest("expected_integrity_digest"))
    check(receipt.commandFingerprint == digest("expected_command_fingerprint"))
    check(receipt.selectionRevision.value == number("expected_committed_selection_revision"))
    check(receipt.chapterCount == fixture.getValue("chapter_count").toUInt())
    check(receipt.adSpanCount == fixture.getValue("ad_span_count").toUInt())
    val chapterId = id("expected_chapter_0_id")
    check(projection.chapters.single().chapterId == ChapterId(chapterId.first, chapterId.second))
    check(projection.chapters.single().title == fixture["chapter_0_expected_title"])
    check(projection.chapters.single().effectiveEndMilliseconds ==
        number("chapter_0_expected_effective_end_milliseconds"))
    check(projection.hasMore)

    val adQualified = projectChapterContract(
        request,
        ChapterProjectionScope.AdSpans,
        0u,
        20u.toUShort(),
    )
    check(adQualified is ChapterContractProjection.Qualified)
    val adId = id("expected_ad_span_0_id")
    check(adQualified.artifact.adSpans.single().adSpanId == AdSpanId(adId.first, adId.second))
    check(adQualified.artifact.adSpans.single().kind == ChapterAdKind.Midroll)
}

fun qualifyChapterObservations() {
    val limits = chapterObservationLimits()
    check(limits.publisherDocumentBytes == 2_097_152UL)
    check(limits.modelPromptBytes == 262_144UL)
    check(limits.modelCompletionBytes == 1_048_576UL)
    check(limits.agentItems == 4_096u)
    check(limits.sourceUrlBytes == 4_096UL)
    check(limits.publisherContentTypeBytes == 512UL)
    check(limits.providerBytes == 128UL)
    check(limits.modelBytes == 256UL)
    val episode = EpisodeId(11UL, 101UL)
    val podcast = PodcastId(12UL, 101UL)
    val observedAt = UnixTimestampMilliseconds(1_721_322_123_456L)
    val publisherBytes = """{"version":"1.2.0","chapters":[
        {"startTime":0,"title":"Publisher opening"},
        {"startTime":50,"title":"Publisher source","toc":false}
    ]}""".toByteArray()
    val publisher = qualifyPublisherChapterObservation(
        PublisherChapterObservation(
            episode,
            podcast,
            "https://example.test/chapters.json",
            "application/json",
            chapterDigest(publisherBytes),
            publisherBytes,
            observedAt,
            100_000UL,
        ),
    )
    check(publisher is ChapterObservationProjection.Qualified)
    check(publisher.artifact.provenance.source == ChapterArtifactSource.Publisher)
    check(publisher.artifact.chapters[1].includeInTableOfContents.not())

    val transcriptDigest = chapterDigest("selected transcript".toByteArray())
    fun modelPlan(artifact: ChapterArtifactInput?): ChapterModelPlan = planChapterModelRequest(
        ChapterModelPlanInput(
            ChapterModelEpisodeInput(
                episode,
                podcast,
                "Kotlin chapter planning",
                "Prompt v1 ignores this description",
                100.0,
            ),
            TranscriptVersionId(13UL, 101UL),
            transcriptDigest,
            ChapterModelTranscriptInput(
                TranscriptVersionId(13UL, 101UL),
                transcriptDigest,
                listOf(
                    ChapterModelTranscriptSegmentInput(0.0, "Opening evidence"),
                    ChapterModelTranscriptSegmentInput(50.0, "Source evidence"),
                ),
            ),
            artifact,
            if (artifact?.provenance?.source == ChapterArtifactSource.Publisher) artifact else publisher.artifact,
            StateRevision(0UL),
            "openai/gpt-4o-mini",
        ),
    )
    val generatedPlan = modelPlan(null)
    check(generatedPlan is ChapterModelPlan.Ready)
    check(generatedPlan.request.provider == "openrouter")
    check(generatedPlan.request.model == "openai/gpt-4o-mini")
    check(generatedPlan.request.responseFormat == ChapterModelResponseFormat.JsonObject)
    check(generatedPlan.request.maximumCompletionBytes == 1_048_576UL)
    check(generatedPlan.request.mode == ChapterModelObservationMode.Generate)
    check(generatedPlan.request.expectedArtifactSource == ChapterArtifactSource.Generated)

    val enrichedPlan = modelPlan(publisher.artifact)
    check(enrichedPlan is ChapterModelPlan.Ready)
    check(enrichedPlan.request.mode is ChapterModelObservationMode.Enrich)
    check(enrichedPlan.request.expectedArtifactSource == ChapterArtifactSource.PublisherEnriched)
    check(enrichedPlan.request.userPrompt.contains("use these exact indices"))
    val desired = planChapterModelDesiredState(
        ChapterModelDesiredStateInput(
            transcriptDigest,
            "openai/gpt-4o-mini",
            ChapterArtifactSource.Generated,
        ),
    )
    check(desired is ChapterModelDesiredStatePlan.Compile)
    check(desired.inputVersion.length == 64)

    val completion = """{"chapters":[
        {"start":5,"title":"Generated one","summary":"One"},
        {"start":25,"title":"Generated two"},
        {"start":50,"title":"Generated three"},
        {"start":75,"title":"Generated four"}
    ],"ads":[]}"""
    fun model(mode: ChapterModelObservationMode, body: String) = ModelChapterObservation(
        episode,
        podcast,
        2u,
        TranscriptVersionId(13UL, 101UL),
        transcriptDigest,
        TranscriptVersionId(13UL, 101UL),
        transcriptDigest,
        1u,
        "model-input-v1",
        "openrouter",
        "fixture-model-v1",
        chapterDigest(body.toByteArray()),
        body,
        observedAt,
        100_000UL,
        mode,
    )
    val generated = qualifyModelChapterObservation(
        model(ChapterModelObservationMode.Generate, completion),
    )
    check(generated is ChapterObservationProjection.Qualified)
    check(generated.artifact.provenance.source == ChapterArtifactSource.Generated)
    check(generated.artifact.chapters.first().startMilliseconds == 0UL)

    val enrichment = """{"summaries":[{"index":0,"summary":"Enriched"}],"ads":[]}"""
    val enriched = qualifyModelChapterObservation(
        model(ChapterModelObservationMode.Enrich(publisher.artifact), enrichment),
    )
    check(enriched is ChapterObservationProjection.Qualified)
    check(enriched.artifact.provenance.source == ChapterArtifactSource.PublisherEnriched)
    check(enriched.artifact.chapters.first().summary == "Enriched")

    val agent = qualifyAgentComposedChapterObservation(
        AgentComposedChapterObservation(
            episode,
            podcast,
            "agent-composition-v1",
            1u,
            "elevenlabs",
            "eleven-multilingual-v2",
            chapterDigest("ordered turns".toByteArray()),
            observedAt,
            30_000UL,
            listOf(
                AgentComposedChapterItem(0.0, 10.25, "Synthesis", null, null, null, true, null),
                AgentComposedChapterItem(
                    10.25,
                    30.0,
                    "Source moment",
                    null,
                    null,
                    "https://example.test/source?t=42",
                    true,
                    EpisodeId(99UL, 7UL),
                ),
            ),
        ),
    )
    check(agent is ChapterObservationProjection.Qualified)
    check(agent.artifact.chapters[1].endMilliseconds == 30_000UL)
    check(agent.artifact.chapters[1].sourceEpisodeId == EpisodeId(99UL, 7UL))

    val rejected = qualifyPublisherChapterObservation(
        PublisherChapterObservation(
            episode,
            podcast,
            "https://example.test/chapters.json",
            "text/html",
            chapterDigest(publisherBytes),
            publisherBytes,
            observedAt,
            100_000UL,
        ),
    )
    check(rejected is ChapterObservationProjection.Rejected)
    check(rejected.reason == ChapterObservationRejection.InvalidContentType)
}

private fun chapterDigest(bytes: ByteArray): ContentDigest {
    val digest = MessageDigest.getInstance("SHA-256").digest(bytes)
    fun word(offset: Int): ULong = (offset until offset + 8).fold(0UL) { value, index ->
        (value shl 8) or digest[index].toUByte().toULong()
    }
    return ContentDigest(word(0), word(8), word(16), word(24))
}
