import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*

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
    check(fixture["contract_version"]?.toUInt() == 14u)
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

fun qualifyChapterMigrationBoundary() {
    val missing = "/definitely-missing-pod0-chapter-source"
    val inspected = inspectLegacyChapterMigration(missing, missing)
    check(inspected.stage == LegacyChapterMigrationStage.BLOCKED)
    check(inspected.failure?.code == LegacyChapterMigrationFailureCode.STORAGE_UNAVAILABLE)
    check(inspected.report == null && inspected.rollbackExport == null)

    val status = readActiveLegacyChapterMigration(missing)
    check(status.stage == LegacyChapterMigrationStage.BLOCKED)
    check(status.failure?.diagnosticCode == "storage_sqlite")

    val rollback = exportLegacyChapterRollback(missing, missing, missing)
    check(rollback.stage == LegacyChapterMigrationStage.BLOCKED)
    check(rollback.rollbackExport == null)
}
