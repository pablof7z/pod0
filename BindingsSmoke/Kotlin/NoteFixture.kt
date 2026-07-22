import uniffi.pod0_application.*
import uniffi.pod0_domain.*
import uniffi.pod0_facade.*
import java.io.File

fun qualifyNoteProjection(fixture: Map<String, String>) {
    fun number(key: String) = fixture.getValue(key).toULong()
    val episodeId = EpisodeId(number("episode_id_high"), number("episode_id_low"))
    val evidence = NoteEvidenceReference(
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
    val note = NoteRecord(
        NoteId(number("note_id_high"), number("note_id_low")),
        NoteRevision(number("note_revision")),
        fixture.getValue("text"),
        NoteKind.Reflection,
        NoteAuthor.User,
        NoteTarget.Episode(episodeId, number("position_milliseconds")),
        UnixTimestampMilliseconds(number("created_at_milliseconds").toLong()),
        fixture.getValue("deleted").toBooleanStrict(),
        evidence,
    )
    val projection = NotesProjection(
        NoteProjectionScope.Episode(episodeId),
        StateRevision(number("collection_revision")),
        listOf(note),
        emptyList(),
        false,
    )

    check(fixture["fixture_version"] == "1")
    check(fixture["contract_version"]?.toUInt() == 31u)
    check(projection.notes.single().text == fixture["text"])
    check(projection.notes.single().evidence?.spanId == evidence.spanId)
}

fun qualifyEmptyNoteImport(source: File, root: File) {
    val coreStore = File(root, "core.sqlite").absolutePath
    val plan = inspectLegacyNoteSource(source.absolutePath)
    check(plan.noteCount == 0u)
    val report = stageLegacyNoteImport(
        source.absolutePath,
        File(root, "notes.backup.json").absolutePath,
        coreStore,
        File(root, "core.backup.sqlite").absolutePath,
        plan,
        CommandId(0UL, 3UL),
        CommandId(0UL, 2UL),
        1_721_322_000_002L,
    )
    check(report.staged && !report.reusedExisting)
    check(readStagedLegacyNoteImport(coreStore, CommandId(0UL, 3UL)).notes.isEmpty())
    check(!commitStagedLegacyNoteImport(coreStore, 1_721_322_000_003L))

}
