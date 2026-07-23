import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*
import java.io.File

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
    check(fixture["contract_version"]?.toUInt() == 44u)
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

fun qualifyEmptyTranscriptImport(coreStore: File, root: File) {
    val transcriptRoot = File(root, "transcripts").apply { mkdirs() }
    val backupRoot = File(root, "transcript-backups")
    val importId = CommandId(0UL, 5UL)
    val plan = inspectLegacyTranscriptSource(coreStore.absolutePath, transcriptRoot.absolutePath)
    check(plan.artifactCount == 0u && plan.selectedCount == 0u)
    val staged = stageLegacyTranscriptImport(
        coreStore.absolutePath,
        transcriptRoot.absolutePath,
        backupRoot.absolutePath,
        coreStore.absolutePath,
        File(root, "core.backup.sqlite").absolutePath,
        plan,
        importId,
        CommandId(0UL, 2UL),
        1_721_322_000_006L,
    )
    check(staged.state == LegacyTranscriptImportState.STAGED)
    val verified = verifyStagedLegacyTranscriptImport(
        coreStore.absolutePath,
        backupRoot.absolutePath,
        importId,
        1_721_322_000_007L,
    )
    check(verified.report.state == LegacyTranscriptImportState.VERIFIED)
    check(verified.verifiedArtifactCount == 0u)
    val committed = commitStagedLegacyTranscriptImport(
        coreStore.absolutePath,
        transcriptRoot.absolutePath,
        coreStore.absolutePath,
        importId,
        1_721_322_000_008L,
    )
    check(committed.state == LegacyTranscriptImportState.COMMITTED)
    check(sharedTranscriptStoreIsAuthoritative(coreStore.absolutePath))
}

fun qualifyTranscriptRuntime(coreStore: File, podcastId: PodcastId, episodeId: EpisodeId) {
    val speakerId = SpeakerId(91UL, 92UL)
    val artifact = TranscriptArtifactInput(
        episodeId,
        podcastId,
        "kotlin-runtime-v1",
        TranscriptSource.Scribe,
        "elevenLabsScribe",
        ContentDigest(1UL, 2UL, 3UL, 4UL),
        "en-US",
        UnixTimestampMilliseconds(1_721_322_000_100L),
        listOf(TranscriptArtifactSpeakerInput(speakerId, "host", "Ada")),
        listOf(
            TranscriptArtifactSegmentInput(
                "Hello Kotlin",
                0UL,
                1_000UL,
                speakerId,
                listOf(
                    TranscriptArtifactWordInput("Hello", 0UL, 400UL),
                    TranscriptArtifactWordInput("Kotlin", 400UL, 1_000UL),
                ),
            ),
            TranscriptArtifactSegmentInput(
                "Bounded projection",
                900UL,
                1_800UL,
                speakerId,
                emptyList(),
            ),
        ),
    )
    val commandId = CommandId(0UL, 41UL)
    val facade = Pod0Facade.open(coreStore.absolutePath)
    try {
        qualifyEmptyKnowledgeRuntime(facade)
        facade.dispatch(CommandEnvelope(
            commandId,
            CancellationId(0UL, 42UL),
            null,
            ApplicationCommand.CommitTranscript(StateRevision(0UL), artifact),
        ))
        val summary = transcriptProjection(
            facade,
            episodeId,
            TranscriptProjectionScope.Summary,
            0u,
            1u.toUShort(),
        )
        check(summary.summary?.selectionRevision == StateRevision(1UL))
        val operation = summary.operations.single { it.commandId == commandId }
        check(operation.stage is OperationStage.Succeeded)
        val result = operation.result
        check(result is OperationResult.TranscriptCommitted)
        check(result.receipt.segmentCount == 2u)
        check(result.receipt.wordCount == 2UL)

        val firstPage = transcriptProjection(
            facade,
            episodeId,
            TranscriptProjectionScope.Segments,
            0u,
            1u.toUShort(),
        )
        check(firstPage.segments.single().text == "Hello Kotlin")
        check(firstPage.hasMore)
        val segmentId = firstPage.segments.single().segmentId
        val exact = transcriptProjection(
            facade,
            episodeId,
            TranscriptProjectionScope.Segment(segmentId),
            0u,
            1u.toUShort(),
        )
        check(exact.segments.single().segmentId == segmentId)
        val words = transcriptProjection(
            facade,
            episodeId,
            TranscriptProjectionScope.Words(segmentId),
            0u,
            1u.toUShort(),
        )
        check(words.words.single().text == "Hello")
        check(words.hasMore)

        val staleId = CommandId(0UL, 43UL)
        facade.dispatch(CommandEnvelope(
            staleId,
            CancellationId(0UL, 44UL),
            null,
            ApplicationCommand.CommitTranscript(StateRevision(0UL), artifact),
        ))
        val stale = transcriptProjection(
            facade,
            episodeId,
            TranscriptProjectionScope.Summary,
            0u,
            1u.toUShort(),
        ).operations.single { it.commandId == staleId }
        check(stale.stage is OperationStage.Failed)
        check(stale.failure?.code is CoreFailureCode.RevisionConflict)
        qualifyModelChapterRuntime(facade, episodeId)
    } finally {
        facade.destroy()
    }

    val reopened = Pod0Facade.open(coreStore.absolutePath)
    try {
        val restored = transcriptProjection(
            reopened,
            episodeId,
            TranscriptProjectionScope.Summary,
            0u,
            1u.toUShort(),
        )
        check(restored.summary?.selectionRevision == StateRevision(1UL))
        check(restored.summary.sourceRevision == "kotlin-runtime-v1")
    } finally {
        reopened.destroy()
    }
}

private fun transcriptProjection(
    facade: Pod0Facade,
    episodeId: EpisodeId,
    scope: TranscriptProjectionScope,
    offset: UInt,
    maxItems: UShort,
): TranscriptProjection {
    val projection = facade.snapshot(ProjectionRequest(
        ProjectionScope.Transcript(episodeId, scope),
        offset,
        maxItems,
    )).projection
    check(projection is Projection.Transcript)
    check(projection.value.failure == null)
    return projection.value
}
