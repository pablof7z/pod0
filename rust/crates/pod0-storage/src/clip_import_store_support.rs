use std::path::Path;

use pod0_domain::{CommandId, StateRevision};
use rusqlite::{Connection, OptionalExtension};

use crate::legacy_clip_source::inspect_clip_source;
use crate::{
    ClipBackupEvidence, ClipImportPlan, ClipImportReport, InspectedClipSource, LegacySourceKind,
    StorageError,
};

pub(crate) fn stored_clip_import_report(
    connection: &Connection,
    import_id: CommandId,
    expected_backup: Option<&ClipBackupEvidence>,
) -> Result<Option<ClipImportReport>, StorageError> {
    let row = connection
        .query_row(
            "SELECT source_kind,source_hash,source_generation,clip_count,backup_byte_count,\
             target_revision FROM pod0_clip_imports WHERE import_id=?1",
            [import_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read clip import", error))?;
    let Some((kind, hash, generation, count, bytes, revision)) = row else {
        return Ok(None);
    };
    let source_kind = LegacySourceKind::from_code(
        u8::try_from(kind).map_err(|_| corrupt("clip source kind is malformed"))?,
    )
    .ok_or_else(|| corrupt("clip source kind is malformed"))?;
    let plan = ClipImportPlan {
        source_kind,
        source_hash: hash,
        source_generation: u64::try_from(generation)
            .map_err(|_| corrupt("clip source generation is malformed"))?,
        clip_count: u32::try_from(count).map_err(|_| corrupt("clip count is malformed"))?,
    };
    let backup = ClipBackupEvidence {
        source_kind,
        source_hash: plan.source_hash.clone(),
        source_generation: plan.source_generation,
        byte_count: u64::try_from(bytes).map_err(|_| corrupt("clip backup bytes are malformed"))?,
        reused_existing: true,
    };
    if let Some(expected) = expected_backup
        && (expected.source_kind != backup.source_kind
            || expected.source_hash != backup.source_hash
            || expected.source_generation != backup.source_generation
            || expected.byte_count != backup.byte_count)
    {
        return Err(StorageError::ImportConflict);
    }
    Ok(Some(ClipImportReport {
        import_id,
        plan,
        target_revision: StateRevision::new(
            u64::try_from(revision).map_err(|_| corrupt("clip import revision is malformed"))?,
        ),
        backup,
        staged: cutover_state(connection)?.as_deref() == Some("staged"),
        reused_existing: true,
    }))
}

pub(crate) fn current_core_revision(
    connection: &Connection,
) -> Result<StateRevision, StorageError> {
    let value: i64 = connection
        .query_row(
            "SELECT state_revision FROM pod0_playback_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read clip import revision", error))?;
    Ok(StateRevision::new(u64::try_from(value).map_err(|_| {
        corrupt("clip import revision is malformed")
    })?))
}

pub(crate) fn require_prerequisites(connection: &Connection) -> Result<(), StorageError> {
    for domain in ["listening", "notes"] {
        let state: Option<String> = connection
            .query_row(
                "SELECT state FROM pod0_domain_cutovers WHERE domain=?1",
                [domain],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| StorageError::sqlite("read clip import prerequisite", error))?;
        if state.as_deref() != Some("authoritative") {
            return Err(StorageError::CutoverNotAuthoritative);
        }
    }
    Ok(())
}

pub(crate) fn clip_import_count(connection: &Connection) -> Result<u32, StorageError> {
    connection
        .query_row("SELECT COUNT(*) FROM pod0_clip_imports", [], |row| {
            row.get(0)
        })
        .map_err(|error| StorageError::sqlite("count clip imports", error))
}

pub(crate) fn clip_count(connection: &Connection) -> Result<u32, StorageError> {
    connection
        .query_row("SELECT COUNT(*) FROM pod0_clips", [], |row| row.get(0))
        .map_err(|error| StorageError::sqlite("count clips", error))
}

pub(crate) fn cutover_state(connection: &Connection) -> Result<Option<String>, StorageError> {
    connection
        .query_row(
            "SELECT state FROM pod0_domain_cutovers WHERE domain='clips'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read clip cutover", error))
}

pub(crate) fn source_still_matches(
    source_path: &Path,
    expected: &ClipImportPlan,
) -> Result<InspectedClipSource, StorageError> {
    let source = inspect_clip_source(source_path)?;
    if source.plan == *expected {
        Ok(source)
    } else {
        Err(StorageError::SourceChanged)
    }
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::CorruptSchema { detail }
}
