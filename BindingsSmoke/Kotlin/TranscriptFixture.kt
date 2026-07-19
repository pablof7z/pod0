import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*

fun qualifyTranscriptContract(fixture: Map<String, String>) {
    fun number(key: String) = fixture.getValue(key).toULong()
    fun id(prefix: String) = number("${prefix}_high") to number("${prefix}_low")
    fun digest(prefix: String) = ContentDigest(
        number("${prefix}_word_0"),
        number("${prefix}_word_1"),
        number("${prefix}_word_2"),
        number("${prefix}_word_3"),
    )
    val speakers = (0 until fixture.getValue("speaker_count").toInt()).map { index ->
        val prefix = "speaker_$index"
        val speakerId = id("${prefix}_id")
        TranscriptArtifactSpeakerInput(
            SpeakerId(speakerId.first, speakerId.second),
            fixture.getValue("${prefix}_label"),
            fixture["${prefix}_display_name"],
        )
    }
    val segments = (0 until fixture.getValue("segment_count").toInt()).map { index ->
        val prefix = "segment_$index"
        val speakerIndex = fixture.getValue("${prefix}_speaker_index").toInt()
        val words = (0 until fixture.getValue("${prefix}_word_count").toInt()).map { wordIndex ->
            val word = "${prefix}_word_$wordIndex"
            TranscriptArtifactWordInput(
                fixture.getValue("${word}_text"),
                number("${word}_start_milliseconds"),
                number("${word}_end_milliseconds"),
            )
        }
        TranscriptArtifactSegmentInput(
            fixture.getValue("${prefix}_text"),
            number("${prefix}_start_milliseconds"),
            number("${prefix}_end_milliseconds"),
            speakers[speakerIndex].speakerId,
            words,
        )
    }
    val commandId = id("command_id")
    val episodeId = id("episode_id")
    val podcastId = id("podcast_id")
    val request = TranscriptCommitRequest(
        CommandId(commandId.first, commandId.second),
        StateRevision(number("expected_selection_revision")),
        TranscriptArtifactInput(
            EpisodeId(episodeId.first, episodeId.second),
            PodcastId(podcastId.first, podcastId.second),
            fixture.getValue("source_revision"),
            TranscriptSource.Unsupported(fixture.getValue("source_wire_code").toUInt()),
            fixture["provider"],
            digest("source_payload_digest"),
            fixture.getValue("language"),
            UnixTimestampMilliseconds(fixture.getValue("generated_at_milliseconds").toLong()),
            speakers,
            segments,
        ),
    )
    val qualifiedSegments = projectTranscriptContract(
        request,
        TranscriptProjectionScope.Segments,
        0u,
        1u.toUShort(),
    )
    check(qualifiedSegments is TranscriptContractProjection.Qualified)
    val receipt = qualifiedSegments.receipt
    val segmentProjection = qualifiedSegments.transcript
    val expectedArtifactId = id("expected_artifact_id")
    val expectedVersionId = id("expected_transcript_version_id")
    check(fixture["fixture_version"] == "1")
    check(fixture["contract_version"]?.toUInt() == 11u)
    check(fixture["unknown_future_field"] == "ignored-by-v1-readers")
    check(receipt.artifactId == TranscriptArtifactId(expectedArtifactId.first, expectedArtifactId.second))
    check(receipt.transcriptVersionId == TranscriptVersionId(expectedVersionId.first, expectedVersionId.second))
    check(receipt.transcriptContentDigest == digest("expected_content_digest"))
    check(receipt.artifactIntegrityDigest == digest("expected_integrity_digest"))
    check(receipt.commandFingerprint == digest("expected_command_fingerprint"))
    check(receipt.selectionRevision.value == number("expected_committed_selection_revision"))
    check(receipt.speakerCount == fixture.getValue("speaker_count").toUInt())
    check(receipt.segmentCount == fixture.getValue("segment_count").toUInt())
    check(receipt.wordCount == number("expected_word_count"))

    val expectedSegmentId = id("expected_segment_0_id")
    check(segmentProjection.segments.single().segmentId ==
        TranscriptSegmentId(expectedSegmentId.first, expectedSegmentId.second))
    check(segmentProjection.segments.single().text == fixture["segment_0_text"])
    check(segmentProjection.hasMore)
    val summarySource = segmentProjection.summary?.source
    check(summarySource is TranscriptSource.Unsupported &&
        summarySource.wireCode == fixture.getValue("source_wire_code").toUInt())

    val secondSegmentId = id("expected_segment_1_id")
    val qualifiedWords = projectTranscriptContract(
        request,
        TranscriptProjectionScope.Words(
            TranscriptSegmentId(secondSegmentId.first, secondSegmentId.second),
        ),
        0u,
        20u.toUShort(),
    )
    check(qualifiedWords is TranscriptContractProjection.Qualified)
    val wordProjection = qualifiedWords.transcript
    check(wordProjection.words.last().endMilliseconds == number("segment_1_word_2_end_milliseconds"))
}
