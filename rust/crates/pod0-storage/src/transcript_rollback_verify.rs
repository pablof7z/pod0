use rusqlite::{Connection, OpenFlags};

use super::*;

pub(super) fn verify_bundle(
    bundle: &Path,
    revision: u64,
    artifacts: &[ExportedTranscript],
) -> Result<(), StorageError> {
    verify_bundle_files(bundle, bundle, revision, artifacts)?;
    let plan = crate::inspect_legacy_transcript_source(
        &bundle.join(SELECTION_DATABASE),
        &bundle.join("transcripts"),
    )?;
    let expected_selected = artifacts.iter().filter(|entry| entry.is_selected).count();
    if plan.source_generation != revision
        || plan.artifact_count as usize != artifacts.len()
        || plan.selected_count as usize != expected_selected
    {
        return Err(StorageError::BackupConflict);
    }
    Ok(())
}

pub(super) fn verify_bundle_files(
    bundle: &Path,
    final_path: &Path,
    revision: u64,
    artifacts: &[ExportedTranscript],
) -> Result<(), StorageError> {
    let manifest_bytes = fs::read(bundle.join(MANIFEST_FILE))
        .map_err(|error| StorageError::io("read transcript rollback manifest", error))?;
    let manifest: RollbackManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| StorageError::BackupConflict)?;
    if manifest.format_version != ROLLBACK_FORMAT_VERSION
        || manifest.core_schema_version != CURRENT_SCHEMA_VERSION
        || manifest.transcript_revision != revision
        || manifest.entries.len() != artifacts.len()
    {
        return Err(StorageError::BackupConflict);
    }
    for (manifest_entry_value, entry) in manifest.entries.iter().zip(artifacts) {
        let relative_path = artifact_relative_path(&entry.artifact);
        let bytes = fs::read(bundle.join(&relative_path))
            .map_err(|error| StorageError::io("read rollback transcript", error))?;
        if *manifest_entry_value
            != manifest_entry(
                &entry.artifact,
                &bytes,
                entry.is_selected,
                path_string(&relative_path)?,
            )
            || bytes != legacy_transcript_bytes(&entry.artifact)?
        {
            return Err(StorageError::BackupConflict);
        }
    }
    verify_selection_database(
        &bundle.join(SELECTION_DATABASE),
        final_path,
        revision,
        artifacts,
    )
}

fn verify_selection_database(
    path: &Path,
    final_path: &Path,
    revision: u64,
    artifacts: &[ExportedTranscript],
) -> Result<(), StorageError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open rollback selection database", error))?;
    crate::backup::verify_connection(&connection)?;
    let generation: String = connection
        .query_row(
            "SELECT CAST(value AS TEXT) FROM persistence_metadata WHERE key='generation'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("verify rollback export revision", error))?;
    if generation != revision.to_string() {
        return Err(StorageError::BackupConflict);
    }
    let mut statement = connection
        .prepare(
            "SELECT subject_id,input_version,output_version,content_hash,location,origin,\
             verified_at,selected FROM artifacts WHERE kind='transcript' ORDER BY subject_id,output_version",
        )
        .map_err(|error| StorageError::sqlite("prepare rollback verification", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, bool>(7)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read rollback verification", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| StorageError::sqlite("decode rollback verification", error))?;
    if rows.len() != artifacts.len() {
        return Err(StorageError::BackupConflict);
    }
    for (row, entry) in rows.iter().zip(artifacts) {
        let artifact = &entry.artifact;
        let expected_location = final_path.join(artifact_relative_path(artifact));
        let bytes = legacy_transcript_bytes(artifact)?;
        if row.0 != uuid_string(artifact.episode_id.into_bytes())
            || row.1 != artifact.source_revision
            || row.2 != hex_id(artifact.artifact_id.into_bytes())
            || row.3 != hex_digest(digest_bytes(&bytes))
            || row.4 != path_string(&expected_location)?
            || row.5 != legacy_source(artifact.provenance.source)?
            || row.6 != artifact.generated_at.value() as f64 / 1_000.0
            || row.7 != entry.is_selected
        {
            return Err(StorageError::BackupConflict);
        }
    }
    Ok(())
}
