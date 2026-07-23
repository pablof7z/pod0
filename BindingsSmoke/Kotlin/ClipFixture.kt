import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*
import java.io.File

fun qualifyClipProjection(fixture: Map<String, String>) {
    fun number(key: String) = fixture.getValue(key).toULong()
    val episodeId = EpisodeId(number("episode_id_high"), number("episode_id_low"))
    val evidence = ClipEvidenceReference(
        EvidenceGenerationId(number("generation_id_high"), number("generation_id_low")),
        TranscriptVersionId(
            number("transcript_version_id_high"),
            number("transcript_version_id_low"),
        ),
        ContentDigest(
            number("content_digest_word_0"),
            number("content_digest_word_1"),
            number("content_digest_word_2"),
            number("content_digest_word_3"),
        ),
        EvidenceSpanId(number("span_id_high"), number("span_id_low")),
    )
    val clip = ClipRecord(
        ClipId(number("clip_id_high"), number("clip_id_low")),
        ClipRevision(number("clip_revision")),
        episodeId,
        PodcastId(number("podcast_id_high"), number("podcast_id_low")),
        number("start_milliseconds"),
        number("end_milliseconds"),
        UnixTimestampMilliseconds(number("created_at_milliseconds").toLong()),
        fixture["caption"],
        SpeakerId(number("speaker_id_high"), number("speaker_id_low")),
        fixture["speaker_label"]?.takeIf(String::isNotEmpty),
        fixture.getValue("frozen_transcript_text"),
        ClipSource.Touch,
        fixture.getValue("deleted").toBooleanStrict(),
        evidence,
    )
    val projection = ClipsProjection(
        ClipProjectionScope.Clip(clip.clipId),
        StateRevision(number("collection_revision")),
        listOf(clip),
        emptyList(),
        false,
    )

    check(fixture["fixture_version"] == "1")
    check(fixture["contract_version"]?.toUInt() == 41u)
    check(fixture["source"] == "touch")
    check(projection.clips.single().frozenTranscriptText == fixture["frozen_transcript_text"])
    check(projection.clips.single().evidence?.spanId == evidence.spanId)
}

fun qualifyEmptyClipImport(source: File, root: File) {
    val coreStore = File(root, "core.sqlite").absolutePath
    val plan = inspectLegacyClipSource(source.absolutePath)
    check(plan.clipCount == 0u)
    val report = stageLegacyClipImport(
        source.absolutePath,
        File(root, "clips.backup.json").absolutePath,
        coreStore,
        File(root, "core.backup.sqlite").absolutePath,
        plan,
        CommandId(0UL, 4UL),
        CommandId(0UL, 2UL),
        1_721_322_000_004L,
    )
    check(report.staged && !report.reusedExisting)
    check(readStagedLegacyClipImport(coreStore, CommandId(0UL, 4UL)).clips.isEmpty())
    check(!commitStagedLegacyClipImport(
        source.absolutePath,
        coreStore,
        1_721_322_000_005L,
    ))

}

fun qualifyEmptyKnowledgeRuntime(facade: Pod0Facade) {
    val clips = facade.snapshot(ProjectionRequest(
        ProjectionScope.Clips(ClipProjectionScope.All),
        0u,
        20u.toUShort(),
    )).projection
    check(clips is Projection.Clips && clips.value.clips.isEmpty())
    val notes = facade.snapshot(ProjectionRequest(
        ProjectionScope.Notes(NoteProjectionScope.All),
        0u,
        20u.toUShort(),
    )).projection
    check(notes is Projection.Notes && notes.value.notes.isEmpty())
}
