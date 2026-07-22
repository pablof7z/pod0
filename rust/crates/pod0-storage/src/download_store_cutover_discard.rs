use std::fs;

use rusqlite::{OptionalExtension, params};

use crate::download_store_artifact_file::pending_artifact_path;
use crate::download_store_cutover::{CUTOVER_DOMAIN, read_authority};
use crate::download_store_request::u64_to_i64;
use crate::{
    DownloadWorkflowAuthorityState, LegacyDownloadCutoverDisposition, LegacyDownloadCutoverInput,
    LibraryStore, StorageError,
};

type StagedWorkflowRow = (Vec<u8>, Vec<u8>, Vec<u8>, String, String, String);

impl LibraryStore {
    pub fn discard_staged_legacy_download_cutover(
        &self,
        input: LegacyDownloadCutoverInput,
    ) -> Result<DownloadWorkflowAuthorityState, StorageError> {
        super::download_store_cutover::validate_input(&input)?;
        let mut artifact_keys = Vec::new();
        self.write(|transaction| {
            match read_authority(transaction)? {
                DownloadWorkflowAuthorityState::Staged { source_generation }
                    if source_generation == input.source_generation => {}
                _ => return Err(StorageError::DownloadWorkflowConflict),
            }
            let count: i64 = transaction
                .query_row("SELECT COUNT(*) FROM pod0_download_workflows", [], |row| row.get(0))
                .map_err(|error| {
                    StorageError::sqlite("count staged download workflows", error)
                })?;
            if usize::try_from(count).ok() != Some(input.entries.len()) {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            for entry in &input.entries {
                let row: Option<StagedWorkflowRow> =
                    transaction
                        .query_row(
                            "SELECT intent_id,attempt_id,command_id,input_version,\
                             enclosure_url,stage FROM pod0_download_workflows WHERE episode_id=?1 \
                             AND cancellation_id=?2",
                            params![
                                entry.episode_id.into_bytes().as_slice(),
                                entry.cancellation_id.into_bytes().as_slice(),
                            ],
                            |row| {
                                Ok((
                                    row.get(0)?,
                                    row.get(1)?,
                                    row.get(2)?,
                                    row.get(3)?,
                                    row.get(4)?,
                                    row.get(5)?,
                                ))
                            },
                        )
                        .optional()
                        .map_err(|error| {
                            StorageError::sqlite("verify staged download workflow", error)
                        })?;
                let expected = (
                    entry.intent_id.into_bytes().to_vec(),
                    entry.attempt_id.into_bytes().to_vec(),
                    entry.command_id.into_bytes().to_vec(),
                    entry.input_version.clone(),
                    entry.enclosure_url.clone(),
                );
                let Some((intent, attempt, command, version, enclosure, stage)) = row else {
                    return Err(StorageError::DownloadWorkflowConflict);
                };
                if (intent, attempt, command, version, enclosure) != expected
                    || !matches!(stage.as_str(), "requested" | "succeeded")
                {
                    return Err(StorageError::DownloadWorkflowConflict);
                }
                let request: Option<Vec<u8>> = transaction
                    .query_row(
                        "SELECT request_id FROM pod0_download_attempts WHERE attempt_id=?1 \
                         AND episode_id=?2 AND intent_id=?3 AND attempt=1",
                        params![
                            entry.attempt_id.into_bytes().as_slice(),
                            entry.episode_id.into_bytes().as_slice(),
                            entry.intent_id.into_bytes().as_slice(),
                        ],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| {
                        StorageError::sqlite("verify staged download attempt", error)
                    })?;
                if request.as_deref() != Some(entry.request_id.into_bytes().as_slice()) {
                    return Err(StorageError::DownloadWorkflowConflict);
                }
                if let LegacyDownloadCutoverDisposition::Available { byte_count, .. } =
                    &entry.disposition
                {
                    let key = format!(
                        "legacy-download:{}:v1",
                        hex(&entry.episode_id.into_bytes())
                    );
                    transaction
                        .execute(
                            "UPDATE pod0_episodes SET download_code=2,download_wire_code=NULL,\
                             download_ref_version=1,download_ref_key=?1,download_byte_count=?2 \
                             WHERE episode_id=?3",
                            params![
                                key,
                                u64_to_i64(*byte_count)?,
                                entry.episode_id.into_bytes().as_slice(),
                            ],
                        )
                        .map_err(|error| {
                            StorageError::sqlite("restore legacy download reference", error)
                        })?;
                    if transaction.changes() != 1 {
                        return Err(StorageError::DownloadWorkflowConflict);
                    }
                }
            }
            let mut statement = transaction
                .prepare(
                    "SELECT artifact_key FROM pod0_download_workflows WHERE artifact_key IS NOT NULL",
                )
                .map_err(|error| {
                    StorageError::sqlite("read staged download artifacts", error)
                })?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| {
                    StorageError::sqlite("query staged download artifacts", error)
                })?;
            for row in rows {
                artifact_keys.push(row.map_err(|error| {
                    StorageError::sqlite("decode staged download artifact", error)
                })?);
            }
            drop(statement);
            transaction
                .execute("DELETE FROM pod0_download_workflows", [])
                .map_err(|error| {
                    StorageError::sqlite("discard staged download workflows", error)
                })?;
            if usize::try_from(transaction.changes()).ok() != Some(input.entries.len()) {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            transaction
                .execute(
                    "DELETE FROM pod0_domain_cutovers WHERE domain=?1 AND state='staged' \
                     AND source_generation=?2",
                    params![CUTOVER_DOMAIN, u64_to_i64(input.source_generation)?],
                )
                .map_err(|error| {
                    StorageError::sqlite("discard staged download cutover marker", error)
                })?;
            if transaction.changes() != 1 {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            Ok(())
        })?;
        for key in artifact_keys {
            let path = self.download_artifact_path(&key)?;
            remove_if_present(&path)?;
        }
        for entry in input.entries {
            remove_if_present(&pending_artifact_path(self.path(), entry.attempt_id))?;
        }
        Ok(DownloadWorkflowAuthorityState::NotStarted)
    }
}

fn remove_if_present(path: &std::path::Path) -> Result<(), StorageError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::io(
            "remove discarded download artifact",
            error,
        )),
    }
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
