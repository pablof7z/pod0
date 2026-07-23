pub(crate) fn failed_migration_status(
    connection: &Connection,
    retry_migration_id: CommandId,
) -> Result<(Option<(u32, u32)>, bool), StorageError> {
    let stored = user_version(connection)?;
    let mut statement = connection
        .prepare(
            "SELECT migration_id,from_version,to_version,diagnostic_code \
             FROM pod0_migration_journal \
             WHERE state='failed' AND to_version>?1 ORDER BY started_at_ms",
        )
        .map_err(|error| StorageError::sqlite("prepare failed migration read", error))?;
    let rows = statement
        .query_map([stored], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read failed migrations", error))?;

    let mut blocking = None;
    let mut retrying_legacy_backup_conflict = false;
    for row in rows {
        let (bytes, from_version, to_version, diagnostic_code) =
            row.map_err(|error| StorageError::sqlite("decode failed migration", error))?;
        let failed_migration_id = command_id(&bytes)?;
        let safe_legacy_retry = diagnostic_code.as_deref() == Some("backup_conflict")
            && from_version == stored
            && to_version == stored.saturating_add(1)
            && failed_migration_id != retry_migration_id;
        if safe_legacy_retry {
            retrying_legacy_backup_conflict = true;
        } else if blocking.is_none() {
            blocking = Some((from_version, to_version));
        }
    }
    Ok((blocking, retrying_legacy_backup_conflict))
}
