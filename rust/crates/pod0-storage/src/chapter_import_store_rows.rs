use pod0_domain::{CommandId, ContentDigest, StateRevision};
use rusqlite::{Transaction, params};

use crate::chapter_store_codec::sqlite_i64;
use crate::{
    ChapterBackupEvidence, ChapterEvidenceKind, InspectedChapterEvidence, InspectedChapterSource,
    LegacyChapterSourceKind, StorageError,
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_import_record(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    fingerprint: ContentDigest,
    source: &InspectedChapterSource,
    backup: &ChapterBackupEvidence,
    target_revision: StateRevision,
    chapter_count: u64,
    ad_span_count: u64,
    staged_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_chapter_imports(import_id,source_kind,source_identity,\
             source_generation,source_byte_count,source_database_digest,source_selection_digest,\
             command_fingerprint,evidence_count,artifact_count,selected_count,blocked_count,\
             chapter_count,ad_span_count,target_revision,state,backup_database_digest,\
             backup_database_byte_count,backup_file_count,backup_file_byte_count,staged_at_ms) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,'staged',\
             ?16,?17,?18,?19,?20)",
            params![
                import_id.into_bytes().as_slice(),
                source.plan.source_kind.code(),
                source.plan.source_file_identity.into_bytes().as_slice(),
                sqlite_i64(source.plan.source_generation)?,
                sqlite_i64(source.plan.source_database_byte_count)?,
                source.plan.source_database_digest.into_bytes().as_slice(),
                source.plan.source_selection_digest.into_bytes().as_slice(),
                fingerprint.into_bytes().as_slice(),
                source.plan.evidence_count,
                source.plan.canonical_artifact_count,
                source.plan.selected_count,
                source.plan.blocked_count,
                sqlite_i64(chapter_count)?,
                sqlite_i64(ad_span_count)?,
                sqlite_i64(target_revision.value)?,
                backup.database_digest.into_bytes().as_slice(),
                sqlite_i64(backup.database_byte_count)?,
                backup.file_count,
                sqlite_i64(backup.file_byte_count)?,
                staged_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("record chapter import", error))?;
    Ok(())
}

pub(crate) fn insert_import_entry(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    source_kind: LegacyChapterSourceKind,
    entry: &InspectedChapterEvidence,
) -> Result<(), StorageError> {
    let artifact_id = entry
        .artifact
        .as_ref()
        .map(|artifact| artifact.artifact_id.into_bytes().to_vec());
    let source_path = entry
        .source_path
        .as_deref()
        .and_then(|path| path.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("database:{}", entry.source_subject));
    transaction
        .execute(
            "INSERT INTO pod0_chapter_import_entries(import_id,entry_id,evidence_kind,\
             source_kind,source_subject,source_input_version,source_output_version,source_origin,\
             source_schema_version,source_integrity,source_verified_at_ms,episode_id,podcast_id,\
             source_row_id,source_row_digest,source_file_path,source_file_digest,\
             source_file_byte_count,raw_digest,raw_byte_count,backup_file_digest,\
             backup_file_byte_count,legacy_selected,importer_selected,validation_state,\
             diagnostic_code,artifact_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,\
             ?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27)",
            params![
                import_id.into_bytes().as_slice(),
                entry.evidence_id.into_bytes().as_slice(),
                entry.kind.code(),
                entry_source_kind(entry.kind, source_kind),
                entry.source_subject,
                bounded_optional(entry.source_input_version.as_deref(), 1_024),
                bounded_optional(entry.source_output_version.as_deref(), 1_024),
                bounded_optional(entry.source_origin.as_deref(), 4_096),
                entry.source_schema_version.unwrap_or(0),
                entry.source_integrity.as_deref().unwrap_or("available"),
                entry.source_verified_at_ms,
                entry.episode_id.map(|value| value.into_bytes().to_vec()),
                entry.podcast_id.map(|value| value.into_bytes().to_vec()),
                entry.source_row_id.map(sqlite_i64).transpose()?,
                entry.source_row_digest.into_bytes().as_slice(),
                source_path,
                entry.raw_digest.into_bytes().as_slice(),
                sqlite_i64(entry.raw_byte_count)?,
                entry.raw_digest.into_bytes().as_slice(),
                sqlite_i64(entry.raw_byte_count)?,
                entry.raw_digest.into_bytes().as_slice(),
                sqlite_i64(entry.raw_byte_count)?,
                entry.legacy_selected,
                entry.importer_selected,
                entry.validation.code(),
                entry.diagnostic_code,
                artifact_id,
            ],
        )
        .map_err(|error| StorageError::sqlite("record chapter import entry", error))?;
    insert_chapter_evidence(transaction, import_id, entry)?;
    insert_ad_evidence(transaction, import_id, entry)
}

fn insert_chapter_evidence(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    entry: &InspectedChapterEvidence,
) -> Result<(), StorageError> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO pod0_chapter_import_chapter_evidence(import_id,entry_id,ordinal,\
             legacy_id,legacy_is_ai_generated,chapter_id) VALUES(?1,?2,?3,?4,?5,?6)",
        )
        .map_err(|error| StorageError::sqlite("prepare chapter import identities", error))?;
    for chapter in &entry.legacy_chapters {
        statement
            .execute(params![
                import_id.into_bytes().as_slice(),
                entry.evidence_id.into_bytes().as_slice(),
                chapter.ordinal,
                chapter.legacy_id.map(|value| value.to_vec()),
                chapter.is_ai_generated,
                chapter.chapter_id.map(|value| value.into_bytes().to_vec()),
            ])
            .map_err(|error| StorageError::sqlite("record chapter import identity", error))?;
    }
    Ok(())
}

fn insert_ad_evidence(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    entry: &InspectedChapterEvidence,
) -> Result<(), StorageError> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO pod0_chapter_import_ad_evidence(import_id,entry_id,ordinal,legacy_id,\
             ad_span_id) VALUES(?1,?2,?3,?4,?5)",
        )
        .map_err(|error| StorageError::sqlite("prepare ad-span import identities", error))?;
    for span in &entry.legacy_ad_spans {
        statement
            .execute(params![
                import_id.into_bytes().as_slice(),
                entry.evidence_id.into_bytes().as_slice(),
                span.ordinal,
                span.legacy_id.map(|value| value.to_vec()),
                span.ad_span_id.map(|value| value.into_bytes().to_vec()),
            ])
            .map_err(|error| StorageError::sqlite("record ad-span import identity", error))?;
    }
    Ok(())
}

fn entry_source_kind(kind: ChapterEvidenceKind, source: LegacyChapterSourceKind) -> &'static str {
    if kind == ChapterEvidenceKind::EpisodeAdjunct {
        "episode_adjunct"
    } else {
        match source {
            LegacyChapterSourceKind::ArtifactSqliteV0 => "workflow_artifact_v0",
            LegacyChapterSourceKind::ArtifactSqliteV1 => "workflow_artifact_v1",
        }
    }
}

fn bounded_optional(value: Option<&str>, maximum: usize) -> Option<&str> {
    value.filter(|value| !value.is_empty() && value.len() <= maximum)
}
