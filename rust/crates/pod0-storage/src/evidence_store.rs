use std::path::{Path, PathBuf};

use pod0_domain::{EpisodeId, EvidenceGenerationId, TranscriptEvidenceArtifact};
use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::evidence_store_read::{read_artifact, read_summary, selected_generation_id};
use crate::migration_db::{configure, open_connection, user_version, validate_open_database};
use crate::{CURRENT_SCHEMA_VERSION, EvidenceGenerationSummary, StorageError};

#[derive(Clone, Debug)]
pub struct EvidenceStore {
    path: PathBuf,
}

impl EvidenceStore {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let connection = open_current(path, true)?;
        require_valid_foreign_keys(&connection)?;
        Ok(Self {
            path: path.to_owned(),
        })
    }

    pub fn generation(
        &self,
        generation_id: EvidenceGenerationId,
    ) -> Result<Option<TranscriptEvidenceArtifact>, StorageError> {
        let connection = open_current(&self.path, true)?;
        read_artifact(&connection, generation_id)
    }

    pub fn generation_summary(
        &self,
        generation_id: EvidenceGenerationId,
    ) -> Result<Option<EvidenceGenerationSummary>, StorageError> {
        let connection = open_current(&self.path, true)?;
        read_summary(&connection, generation_id)
    }

    pub fn selected_generation(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<EvidenceGenerationSummary>, StorageError> {
        let connection = open_current(&self.path, true)?;
        let Some(generation_id) = selected_generation_id(&connection, episode_id)? else {
            return Ok(None);
        };
        let summary = read_summary(&connection, generation_id)?
            .ok_or(StorageError::InvalidEvidenceArtifact)?;
        if summary.episode_id != episode_id {
            return Err(StorageError::EvidenceEpisodeMismatch);
        }
        Ok(Some(summary))
    }

    pub fn selected_artifact(
        &self,
        episode_id: EpisodeId,
    ) -> Result<Option<TranscriptEvidenceArtifact>, StorageError> {
        let connection = open_current(&self.path, true)?;
        let Some(generation_id) = selected_generation_id(&connection, episode_id)? else {
            return Ok(None);
        };
        let artifact = read_artifact(&connection, generation_id)?
            .ok_or(StorageError::InvalidEvidenceArtifact)?;
        if artifact.version.episode_id != episode_id {
            return Err(StorageError::EvidenceEpisodeMismatch);
        }
        Ok(Some(artifact))
    }

    pub(crate) fn write<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = open_current(&self.path, false)?;
        configure(&connection)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| StorageError::sqlite("begin evidence command", error))?;
        let output = operation(&transaction)?;
        transaction
            .commit()
            .map_err(|error| StorageError::sqlite("commit evidence command", error))?;
        Ok(output)
    }
}

fn open_current(path: &Path, read_only: bool) -> Result<Connection, StorageError> {
    let connection = open_connection(path, read_only)?;
    let version = user_version(&connection)?;
    validate_open_database(&connection, version)?;
    if version != CURRENT_SCHEMA_VERSION {
        return Err(StorageError::CorruptSchema {
            detail: "evidence store schema is not current",
        });
    }
    Ok(connection)
}

fn require_valid_foreign_keys(connection: &Connection) -> Result<(), StorageError> {
    let violation: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|error| StorageError::sqlite("verify evidence references", error))?;
    if violation.is_none() {
        Ok(())
    } else {
        Err(StorageError::InvalidEvidenceArtifact)
    }
}

use rusqlite::OptionalExtension;
