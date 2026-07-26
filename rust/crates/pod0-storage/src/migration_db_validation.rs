pub(crate) fn validate_current_database_identity(
    connection: &Connection,
    version: u32,
) -> Result<(), StorageError> {
    if version > CURRENT_SCHEMA_VERSION {
        return Err(StorageError::NewerSchema {
            stored: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "core store schema is not current",
        });
    }
    if application_id(connection)? != APPLICATION_ID {
        return Err(StorageError::ForeignDatabase);
    }
    Ok(())
}
