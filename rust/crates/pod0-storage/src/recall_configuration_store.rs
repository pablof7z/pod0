use crate::StorageError;
use crate::library_store::{LibraryStore, command_was_applied, finish_command};
use crate::listening_db_codec::bool_value;
use pod0_domain::{
    CommandId, ContentDigest, RecallConfiguration, RecallConfigurationInput,
    RecallConfigurationOrigin, RecallEmbeddingProvider, RecallRerankProvider, StateRevision,
};
use rusqlite::{OptionalExtension, Row, Transaction};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallConfigurationMutation {
    pub configuration: RecallConfiguration,
    pub changed: bool,
    pub imported: bool,
}
impl LibraryStore {
    pub fn recall_configuration(&self) -> Result<Option<RecallConfiguration>, StorageError> {
        self.read(|connection| {
            connection
                .query_row(RECALL_CONFIGURATION_SELECT, [], decode_configuration)
                .optional()
                .map_err(|error| StorageError::sqlite("read recall configuration", error))?
                .map(validate_stored_configuration)
                .transpose()
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn import_legacy_recall_configuration(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        input: RecallConfigurationInput,
        source_generation: ContentDigest,
        observed_at_ms: i64,
    ) -> Result<RecallConfigurationMutation, StorageError> {
        self.write(|transaction| {
            if let Some(existing) = read_configuration(transaction)? {
                return Ok(RecallConfigurationMutation {
                    configuration: existing,
                    changed: false,
                    imported: false,
                });
            }
            let revision =
                finish_command(transaction, command_id, command_fingerprint, observed_at_ms)?;
            let configuration = RecallConfiguration::legacy_or_default(input, revision);
            insert_configuration(
                transaction,
                &configuration,
                Some(source_generation),
                observed_at_ms,
            )?;
            Ok(RecallConfigurationMutation {
                configuration,
                changed: true,
                imported: true,
            })
        })
    }

    pub fn set_recall_configuration(
        &self,
        command_id: CommandId,
        command_fingerprint: &str,
        expected_revision: StateRevision,
        input: RecallConfigurationInput,
        observed_at_ms: i64,
    ) -> Result<RecallConfigurationMutation, StorageError> {
        self.write(|transaction| {
            if command_was_applied(transaction, command_id, command_fingerprint)?.is_some() {
                let configuration = read_configuration(transaction)?
                    .ok_or(StorageError::RecallConfigurationNotFound)?;
                return Ok(RecallConfigurationMutation {
                    configuration,
                    changed: false,
                    imported: false,
                });
            }
            let stored = read_configuration(transaction)?;
            let current = stored.clone().unwrap_or_default();
            if current.revision != expected_revision {
                return Err(StorageError::RevisionConflict);
            }
            let candidate = RecallConfiguration::validate(
                input,
                StateRevision::INITIAL,
                RecallConfigurationOrigin::User,
            )
            .map_err(|_| StorageError::InvalidRecallConfiguration)?;
            if candidate.input() == current.input() {
                return Ok(RecallConfigurationMutation {
                    configuration: current,
                    changed: false,
                    imported: false,
                });
            }
            let revision =
                finish_command(transaction, command_id, command_fingerprint, observed_at_ms)?;
            let configuration = RecallConfiguration::validate(
                candidate.input(),
                revision,
                RecallConfigurationOrigin::User,
            )
            .map_err(|_| StorageError::InvalidRecallConfiguration)?;
            if stored.is_some() {
                update_configuration(transaction, &configuration, observed_at_ms)?;
            } else {
                insert_configuration(transaction, &configuration, None, observed_at_ms)?;
            }
            Ok(RecallConfigurationMutation {
                configuration,
                changed: true,
                imported: false,
            })
        })
    }
}
const RECALL_CONFIGURATION_SELECT: &str = "SELECT schema_version,revision,origin,embedding_provider,embedding_model,\
     stored_embedding_model_id,embedding_dimensions,embedding_space_digest,\
     reranker_enabled,reranker_provider,reranker_model \
     FROM pod0_recall_configuration WHERE singleton=1";
fn read_configuration(
    transaction: &Transaction<'_>,
) -> Result<Option<RecallConfiguration>, StorageError> {
    transaction
        .query_row(RECALL_CONFIGURATION_SELECT, [], decode_configuration)
        .optional()
        .map_err(|error| StorageError::sqlite("read recall configuration", error))?
        .map(validate_stored_configuration)
        .transpose()
}
fn decode_configuration(row: &Row<'_>) -> rusqlite::Result<StoredRecallConfiguration> {
    Ok(StoredRecallConfiguration {
        schema_version: row.get(0)?,
        revision: row.get(1)?,
        origin: row.get(2)?,
        embedding_provider: row.get(3)?,
        embedding_model: row.get(4)?,
        stored_embedding_model_id: row.get(5)?,
        embedding_dimensions: row.get(6)?,
        embedding_space_digest: row.get(7)?,
        reranker_enabled: row.get(8)?,
        reranker_provider: row.get(9)?,
        reranker_model: row.get(10)?,
    })
}

fn insert_configuration(
    transaction: &Transaction<'_>,
    value: &RecallConfiguration,
    source_generation: Option<ContentDigest>,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    transaction
        .execute(
            "INSERT INTO pod0_recall_configuration(\
         singleton,schema_version,revision,origin,embedding_provider,embedding_model,\
         stored_embedding_model_id,embedding_dimensions,embedding_space_digest,\
         reranker_enabled,reranker_provider,reranker_model,source_generation,updated_at_ms) \
         VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            configuration_params(value, source_generation, observed_at_ms),
        )
        .map_err(|error| StorageError::sqlite("insert recall configuration", error))?;
    Ok(())
}

fn update_configuration(
    transaction: &Transaction<'_>,
    value: &RecallConfiguration,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let changed = transaction
        .execute(
            "UPDATE pod0_recall_configuration SET schema_version=?1,revision=?2,origin=?3,\
         embedding_provider=?4,embedding_model=?5,stored_embedding_model_id=?6,\
         embedding_dimensions=?7,embedding_space_digest=?8,reranker_enabled=?9,\
         reranker_provider=?10,reranker_model=?11,source_generation=?12,updated_at_ms=?13 \
         WHERE singleton=1",
            configuration_params(value, None, observed_at_ms),
        )
        .map_err(|error| StorageError::sqlite("update recall configuration", error))?;
    if changed == 1 {
        Ok(())
    } else {
        Err(StorageError::RecallConfigurationNotFound)
    }
}

fn configuration_params(
    value: &RecallConfiguration,
    source_generation: Option<ContentDigest>,
    observed_at_ms: i64,
) -> [rusqlite::types::Value; 13] {
    use rusqlite::types::Value;
    [
        Value::Integer(i64::from(value.schema_version)),
        Value::Integer(i64::try_from(value.revision.value).expect("bounded revision")),
        Value::Text(origin_code(value.origin).to_owned()),
        Value::Text(embedding_provider_code(value.embedding_provider).to_owned()),
        Value::Text(value.embedding_model.clone()),
        Value::Text(value.stored_embedding_model_id.clone()),
        Value::Integer(i64::from(value.embedding_dimensions)),
        Value::Blob(value.embedding_space_id.into_bytes().to_vec()),
        Value::Integer(bool_value(value.reranker_enabled)),
        value
            .reranker_provider
            .map(reranker_provider_code)
            .map(str::to_owned)
            .map(Value::Text)
            .unwrap_or(Value::Null),
        value
            .reranker_model
            .clone()
            .map(Value::Text)
            .unwrap_or(Value::Null),
        source_generation
            .map(ContentDigest::into_bytes)
            .map(|bytes| Value::Blob(bytes.to_vec()))
            .unwrap_or(Value::Null),
        Value::Integer(observed_at_ms),
    ]
}

struct StoredRecallConfiguration {
    schema_version: u32,
    revision: i64,
    origin: String,
    embedding_provider: String,
    embedding_model: String,
    stored_embedding_model_id: String,
    embedding_dimensions: u16,
    embedding_space_digest: Vec<u8>,
    reranker_enabled: i64,
    reranker_provider: Option<String>,
    reranker_model: Option<String>,
}

fn validate_stored_configuration(
    stored: StoredRecallConfiguration,
) -> Result<RecallConfiguration, StorageError> {
    let revision = StateRevision::new(u64::try_from(stored.revision).map_err(|_| corrupt())?);
    let origin = match stored.origin.as_str() {
        "legacy_swift" => RecallConfigurationOrigin::LegacySwift,
        "user" => RecallConfigurationOrigin::User,
        _ => return Err(corrupt()),
    };
    let value = RecallConfiguration::validate(
        RecallConfigurationInput {
            stored_embedding_model_id: stored.stored_embedding_model_id.clone(),
            reranker_enabled: stored.reranker_enabled == 1,
        },
        revision,
        origin,
    )
    .map_err(|_| corrupt())?;
    if stored.schema_version != value.schema_version
        || stored.embedding_provider != embedding_provider_code(value.embedding_provider)
        || stored.embedding_model != value.embedding_model
        || stored.embedding_dimensions != value.embedding_dimensions
        || stored.embedding_space_digest != value.embedding_space_id.into_bytes()
        || stored.reranker_provider.as_deref()
            != value.reranker_provider.map(reranker_provider_code)
        || stored.reranker_model != value.reranker_model
    {
        return Err(corrupt());
    }
    Ok(value)
}

fn origin_code(value: RecallConfigurationOrigin) -> &'static str {
    match value {
        RecallConfigurationOrigin::LegacySwift => "legacy_swift",
        RecallConfigurationOrigin::User => "user",
        RecallConfigurationOrigin::Default | RecallConfigurationOrigin::Unsupported { .. } => {
            "invalid"
        }
    }
}

fn embedding_provider_code(value: RecallEmbeddingProvider) -> &'static str {
    match value {
        RecallEmbeddingProvider::OpenRouter => "openrouter",
        RecallEmbeddingProvider::Ollama => "ollama",
        RecallEmbeddingProvider::Unsupported { .. } => "unsupported",
    }
}

fn reranker_provider_code(value: RecallRerankProvider) -> &'static str {
    match value {
        RecallRerankProvider::OpenRouter => "openrouter",
        RecallRerankProvider::Unsupported { .. } => "unsupported",
    }
}

fn corrupt() -> StorageError {
    StorageError::CorruptSchema {
        detail: "recall configuration is malformed",
    }
}
