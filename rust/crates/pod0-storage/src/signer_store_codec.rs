fn read_account(connection: &Connection) -> Result<Option<SignerAccountRecord>, StorageError> {
    let row = connection
        .query_row(
            "SELECT account_id,credential_kind_code,credential_kind_wire_code,\
             expected_author_hex,state_revision,stage_code,updated_at_ms,safe_detail \
             FROM pod0_signer_state WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<Vec<u8>>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<u32>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read signer state", error))?
        .ok_or(StorageError::InvalidSignerState)?;
    if row.5 == "unconfigured" {
        return Ok(None);
    }
    let account_id = row
        .0
        .map(|bytes| {
            bytes
                .try_into()
                .map(SignerAccountId::from_bytes)
                .map_err(|_| StorageError::InvalidSignerState)
        })
        .transpose()?;
    Ok(Some(SignerAccountRecord {
        account_id,
        credential_kind: decode_credential(row.1.as_deref(), row.2)?,
        expected_author_hex: row.3,
        revision: decoded_revision(row.4)?,
        stage: decode_stage(&row.5)?,
        updated_at: UnixTimestampMilliseconds::new(row.6),
        safe_detail: row.7,
    }))
}

fn read_revision(connection: &Connection) -> Result<StateRevision, StorageError> {
    connection
        .query_row(
            "SELECT state_revision FROM pod0_signer_state WHERE singleton=1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| StorageError::sqlite("read signer revision", error))
        .and_then(decoded_revision)
}

fn decode_credential(
    code: Option<&str>,
    wire: Option<u32>,
) -> Result<SignerCredentialKind, StorageError> {
    match code {
        Some("local_keychain") => Ok(SignerCredentialKind::LocalKeychain),
        Some("remote_nip46") => Ok(SignerCredentialKind::RemoteNip46),
        Some("unsupported") => Ok(SignerCredentialKind::Unsupported {
            wire_code: wire.ok_or(StorageError::InvalidSignerState)?,
        }),
        _ => Err(StorageError::InvalidSignerState),
    }
}

fn decode_stage(code: &str) -> Result<SignerStage, StorageError> {
    match code {
        "provisioning" => Ok(SignerStage::Provisioning),
        "restoring" => Ok(SignerStage::Restoring),
        "ready" => Ok(SignerStage::Ready),
        "unavailable" => Ok(SignerStage::Unavailable),
        "signing_out" => Ok(SignerStage::SigningOut),
        "failed" => Ok(SignerStage::Failed),
        _ => Err(StorageError::InvalidSignerState),
    }
}

const fn stage_code(stage: SignerStage) -> &'static str {
    match stage {
        SignerStage::Provisioning => "provisioning",
        SignerStage::Restoring => "restoring",
        SignerStage::Ready => "ready",
        SignerStage::Unavailable => "unavailable",
        SignerStage::SigningOut => "signing_out",
        SignerStage::Failed => "failed",
    }
}

fn next_revision(current: StateRevision) -> Result<StateRevision, StorageError> {
    current
        .value
        .checked_add(1)
        .map(StateRevision::new)
        .ok_or(StorageError::SignerConflict)
}

fn stored_revision(revision: StateRevision) -> Result<i64, StorageError> {
    i64::try_from(revision.value).map_err(|_| StorageError::SignerConflict)
}

fn decoded_revision(revision: i64) -> Result<StateRevision, StorageError> {
    u64::try_from(revision)
        .map(StateRevision::new)
        .map_err(|_| StorageError::InvalidSignerState)
}

fn valid_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "signer store schema is not current",
        });
    }
    Ok(connection)
}
