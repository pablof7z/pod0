import uniffi.pod0_application.*
import uniffi.pod0_domain.*

fun qualifyRecallProjection(fixture: Map<String, String>) {
    fun number(key: String) = fixture.getValue(key).toULong()
    fun id(prefix: String) = number("${prefix}_high") to number("${prefix}_low")
    fun digest(prefix: String) = ContentDigest(
        number("${prefix}_word_0"),
        number("${prefix}_word_1"),
        number("${prefix}_word_2"),
        number("${prefix}_word_3"),
    )

    val (episodeHigh, episodeLow) = id("episode_id")
    val (podcastHigh, podcastLow) = id("podcast_id")
    val (generationHigh, generationLow) = id("generation_id")
    val (versionHigh, versionLow) = id("transcript_version_id")
    val (spanHigh, spanLow) = id("span_id")
    val (firstHigh, firstLow) = id("first_segment_id")
    val (lastHigh, lastLow) = id("last_segment_id")
    val (speakerHigh, speakerLow) = id("speaker_id")
    val evidence = RecallEvidenceProjection(
        EpisodeId(episodeHigh, episodeLow),
        PodcastId(podcastHigh, podcastLow),
        EvidenceGenerationId(generationHigh, generationLow),
        TranscriptVersionId(versionHigh, versionLow),
        digest("content_digest"),
        EvidenceSpanId(spanHigh, spanLow),
        TranscriptSegmentId(firstHigh, firstLow),
        TranscriptSegmentId(lastHigh, lastLow),
        number("start_segment_ordinal").toUInt(),
        number("end_segment_ordinal_exclusive").toUInt(),
        number("start_milliseconds"),
        number("end_milliseconds"),
        fixture.getValue("excerpt"),
        SpeakerId(speakerHigh, speakerLow),
        TranscriptProvenance(
            TranscriptSource.Publisher,
            fixture["provenance_provider"],
            digest("source_digest"),
        ),
        RecallScoreProjection(
            number("vector_rrf_units"),
            number("lexical_rrf_units"),
            number("total_rrf_units"),
            number("base_rank").toUShort(),
            number("rerank_rank").toUShort(),
        ),
    )
    val (queryHigh, queryLow) = id("query_id")
    val projection = RecallResultProjection(
        RecallQueryId(queryHigh, queryLow),
        RecallStage.Ready,
        listOf(evidence),
        null,
        null,
    )

    check(fixture["fixture_version"] == "1")
    check(fixture["contract_version"]?.toUInt() == 16u)
    check(fixture["stage"] == "ready")
    check(projection.stage == RecallStage.Ready)
    check(projection.evidence.single().excerpt == fixture["excerpt"])
    check(
        projection.evidence.single().score.totalRrfUnits ==
            projection.evidence.single().score.vectorRrfUnits +
            projection.evidence.single().score.lexicalRrfUnits,
    )
}
