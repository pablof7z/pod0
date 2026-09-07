use pod0_application::{
    SettingsChangeSource, SettingsConflictEvidence, SettingsConflictWinner, SettingsValidationState,
};
use pod0_domain::{
    CommandId, ContentDigest, PRODUCT_SETTINGS_SCHEMA_VERSION, ProductSettings,
    ProductSettingsValues, SettingsWriterVersion, StateRevision,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use crate::{CommitReceipt, LibraryStore, StorageError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsCommitOutcome {
    pub settings: Option<ProductSettings>,
    pub authoritative: bool,
    pub validation: SettingsValidationState,
    pub conflict: Option<SettingsConflictEvidence>,
    pub changed: bool,
    pub receipt: CommitReceipt,
}

impl LibraryStore {
    pub fn product_settings(&self) -> Result<Option<ProductSettings>, StorageError> {
        self.read(read_settings)
    }

    pub fn product_settings_is_authoritative(&self) -> Result<bool, StorageError> {
        self.read(settings_authoritative)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn import_legacy_product_settings(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        source_generation: u64,
        writer_id: ContentDigest,
        values: ProductSettingsValues,
        observed_at_ms: i64,
    ) -> Result<SettingsCommitOutcome, StorageError> {
        crate::transition_commit::commit_product_settings_change(
            self.path(),
            command_id,
            fingerprint,
            pod0_application::SettingsChange::LegacyImport {
                source_generation,
                writer_id,
                values,
            },
            observed_at_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_local_product_settings(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        expected_revision: StateRevision,
        writer_id: ContentDigest,
        values: ProductSettingsValues,
        observed_at_ms: i64,
    ) -> Result<SettingsCommitOutcome, StorageError> {
        crate::transition_commit::commit_product_settings_change(
            self.path(),
            command_id,
            fingerprint,
            pod0_application::SettingsChange::Local {
                expected_revision,
                writer_id,
                values,
            },
            observed_at_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn merge_remote_product_settings(
        &self,
        command_id: CommandId,
        fingerprint: ContentDigest,
        schema_version: u32,
        writer_version: SettingsWriterVersion,
        values: ProductSettingsValues,
        observed_at_ms: i64,
    ) -> Result<SettingsCommitOutcome, StorageError> {
        crate::transition_commit::commit_product_settings_change(
            self.path(),
            command_id,
            fingerprint,
            pod0_application::SettingsChange::Remote {
                schema_version,
                writer_version,
                values,
            },
            observed_at_ms,
        )
    }
}

pub(crate) fn settings_authoritative(connection: &Connection) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT state='authoritative' FROM pod0_domain_cutovers WHERE domain='product_settings'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|error| StorageError::sqlite("read product settings authority", error))
}

pub(crate) fn read_settings(
    connection: &Connection,
) -> Result<Option<ProductSettings>, StorageError> {
    connection
        .query_row(
            "SELECT schema_version,revision,writer_counter,writer_id,values_json \
         FROM pod0_product_settings WHERE singleton=1",
            [],
            decode_settings,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read product settings", error))?
        .map(validate_settings)
        .transpose()
}

pub(crate) fn write_settings(
    transaction: &Transaction<'_>,
    value: &ProductSettings,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let revision = stored_u64(value.revision.value)?;
    let counter = stored_u64(value.writer_version.counter)?;
    let values = serde_json::to_string(&value.values).map_err(|_| StorageError::InvalidActivity)?;
    transaction.execute(
        "INSERT INTO pod0_product_settings(singleton,schema_version,revision,writer_counter,writer_id,\
         values_json,created_at_ms,updated_at_ms) VALUES(1,?1,?2,?3,?4,?5,?6,?6) \
         ON CONFLICT(singleton) DO UPDATE SET schema_version=excluded.schema_version,\
         revision=excluded.revision,writer_counter=excluded.writer_counter,\
         writer_id=excluded.writer_id,values_json=excluded.values_json,updated_at_ms=excluded.updated_at_ms",
        params![value.schema_version, revision, counter,
            value.writer_version.writer_id.into_bytes().as_slice(), values, observed_at_ms],
    ).map_err(|error| StorageError::sqlite("write product settings", error))?;
    Ok(())
}

pub(crate) fn write_validation(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    source: SettingsChangeSource,
    schema_version: u32,
    version: SettingsWriterVersion,
    validation: SettingsValidationState,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let json = serde_json::to_string(&validation).map_err(|_| StorageError::InvalidActivity)?;
    transaction
        .execute(
            "INSERT INTO pod0_settings_validation_evidence(command_id,source_code,\
         candidate_schema_version,candidate_counter,writer_id,validation_json,observed_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                command_id.into_bytes().as_slice(),
                source_code(source),
                schema_version,
                stored_u64(version.counter)?,
                version.writer_id.into_bytes().as_slice(),
                json,
                observed_at_ms
            ],
        )
        .map_err(|error| StorageError::sqlite("write settings validation evidence", error))?;
    Ok(())
}

pub(crate) fn write_conflict(
    transaction: &Transaction<'_>,
    command_id: CommandId,
    value: SettingsConflictEvidence,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO pod0_settings_sync_conflicts(command_id,current_counter,current_writer_id,\
         candidate_counter,candidate_writer_id,current_digest,candidate_digest,winner_code,observed_at_ms) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![command_id.into_bytes().as_slice(), stored_u64(value.current_version.counter)?,
            value.current_version.writer_id.into_bytes().as_slice(),
            stored_u64(value.candidate_version.counter)?,
            value.candidate_version.writer_id.into_bytes().as_slice(),
            value.current_digest.into_bytes().as_slice(), value.candidate_digest.into_bytes().as_slice(),
            winner_code(value.winner), observed_at_ms],
    ).map_err(|error| StorageError::sqlite("write settings conflict evidence", error))?;
    Ok(())
}

pub(crate) fn read_validation(
    connection: &Connection,
    command_id: CommandId,
) -> Result<SettingsValidationState, StorageError> {
    let json: String = connection
        .query_row(
            "SELECT validation_json FROM pod0_settings_validation_evidence WHERE command_id=?1",
            [command_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read settings validation evidence", error))?;
    serde_json::from_str(&json).map_err(|_| StorageError::InvalidActivity)
}

pub(crate) fn read_conflict(
    connection: &Connection,
    command_id: CommandId,
) -> Result<Option<SettingsConflictEvidence>, StorageError> {
    connection.query_row(
        "SELECT current_counter,current_writer_id,candidate_counter,candidate_writer_id,\
         current_digest,candidate_digest,winner_code FROM pod0_settings_sync_conflicts WHERE command_id=?1",
        [command_id.into_bytes().as_slice()], decode_conflict,
    ).optional().map_err(|error| StorageError::sqlite("read settings conflict evidence", error))
}

fn decode_settings(row: &Row<'_>) -> rusqlite::Result<StoredSettings> {
    Ok(StoredSettings {
        schema: row.get(0)?,
        revision: row.get(1)?,
        counter: row.get(2)?,
        writer: row.get(3)?,
        values: row.get(4)?,
    })
}

struct StoredSettings {
    schema: i64,
    revision: i64,
    counter: i64,
    writer: Vec<u8>,
    values: String,
}

fn validate_settings(value: StoredSettings) -> Result<ProductSettings, StorageError> {
    let settings = ProductSettings {
        schema_version: u32::try_from(value.schema).map_err(|_| StorageError::InvalidActivity)?,
        revision: StateRevision::new(loaded_u64(value.revision)?),
        writer_version: SettingsWriterVersion {
            counter: loaded_u64(value.counter)?,
            writer_id: digest(&value.writer)?,
        },
        values: serde_json::from_str(&value.values).map_err(|_| StorageError::InvalidActivity)?,
    };
    if settings.schema_version != PRODUCT_SETTINGS_SCHEMA_VERSION
        || pod0_application::validate_product_settings(&settings.values)
            != SettingsValidationState::Valid
    {
        return Err(StorageError::InvalidActivity);
    }
    Ok(settings)
}

fn decode_conflict(row: &Row<'_>) -> rusqlite::Result<SettingsConflictEvidence> {
    Ok(SettingsConflictEvidence {
        current_version: SettingsWriterVersion {
            counter: row.get::<_, i64>(0)? as u64,
            writer_id: digest_sql(row.get::<_, Vec<u8>>(1)?)?,
        },
        candidate_version: SettingsWriterVersion {
            counter: row.get::<_, i64>(2)? as u64,
            writer_id: digest_sql(row.get::<_, Vec<u8>>(3)?)?,
        },
        current_digest: digest_sql(row.get(4)?)?,
        candidate_digest: digest_sql(row.get(5)?)?,
        winner: match row.get::<_, u8>(6)? {
            0 => SettingsConflictWinner::Current,
            1 => SettingsConflictWinner::Candidate,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
    })
}

fn digest(bytes: &[u8]) -> Result<ContentDigest, StorageError> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| StorageError::InvalidActivity)?;
    Ok(ContentDigest::from_bytes(bytes))
}
fn digest_sql(bytes: Vec<u8>) -> rusqlite::Result<ContentDigest> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(ContentDigest::from_bytes(bytes))
}
fn stored_u64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidActivity)
}
fn loaded_u64(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::InvalidActivity)
}
const fn source_code(value: SettingsChangeSource) -> u8 {
    match value {
        SettingsChangeSource::LegacyImport => 0,
        SettingsChangeSource::Local => 1,
        SettingsChangeSource::Remote => 2,
    }
}
const fn winner_code(value: SettingsConflictWinner) -> u8 {
    match value {
        SettingsConflictWinner::Current => 0,
        SettingsConflictWinner::Candidate => 1,
    }
}
