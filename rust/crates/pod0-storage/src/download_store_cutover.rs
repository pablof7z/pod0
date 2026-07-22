use std::collections::BTreeSet;

use rusqlite::{OptionalExtension, params};

use crate::download_store_cutover_entry::{insert_entry, prepare_entry};
use crate::download_store_request::u64_to_i64;
use crate::{
    DownloadWorkflowAuthorityState, LegacyDownloadCutoverDisposition, LegacyDownloadCutoverInput,
    LegacyDownloadCutoverReport, LibraryStore, StorageError, StoredDownloadStage,
};

pub(crate) const CUTOVER_DOMAIN: &str = "download_workflows";
const MAX_CUTOVER_ENTRIES: usize = 20_000;

impl LibraryStore {
    pub fn download_workflow_authority(
        &self,
    ) -> Result<DownloadWorkflowAuthorityState, StorageError> {
        self.read(read_authority)
    }

    pub fn stage_legacy_download_cutover(
        &self,
        input: LegacyDownloadCutoverInput,
    ) -> Result<LegacyDownloadCutoverReport, StorageError> {
        validate_input(&input)?;
        match self.download_workflow_authority()? {
            DownloadWorkflowAuthorityState::NotStarted => {}
            DownloadWorkflowAuthorityState::Staged { source_generation }
                if source_generation == input.source_generation =>
            {
                let _ = self.recover_download_artifacts()?;
                return self.download_cutover_report();
            }
            DownloadWorkflowAuthorityState::Authoritative { source_generation }
                if source_generation == input.source_generation =>
            {
                return self.download_cutover_report();
            }
            _ => return Err(StorageError::DownloadWorkflowConflict),
        }

        let prepared = input
            .entries
            .iter()
            .cloned()
            .map(|entry| prepare_entry(self, entry))
            .collect::<Result<Vec<_>, _>>()?;
        let repaired_invalid = prepared
            .iter()
            .filter(|entry| entry.repaired_invalid)
            .count();
        self.write(|transaction| {
            if read_authority(transaction)? != DownloadWorkflowAuthorityState::NotStarted {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            let existing: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pod0_download_workflows LIMIT 1)",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| StorageError::sqlite("inspect downloads before cutover", error))?;
            if existing {
                return Err(StorageError::DownloadWorkflowConflict);
            }
            for item in &prepared {
                insert_entry(transaction, &input, item)?;
            }
            transaction
                .execute(
                    "INSERT INTO pod0_domain_cutovers(domain,state,source_generation,\
                     core_revision,committed_at_ms) VALUES(?1,'staged',?2,?3,?4)",
                    params![
                        CUTOVER_DOMAIN,
                        u64_to_i64(input.source_generation)?,
                        u64_to_i64(input.issued_revision.value)?,
                        input.now_ms,
                    ],
                )
                .map_err(|error| StorageError::sqlite("stage download cutover", error))?;
            Ok(())
        })?;
        let _ = self.recover_download_artifacts()?;
        let mut report = self.download_cutover_report()?;
        report.repaired_invalid =
            u32::try_from(repaired_invalid).map_err(|_| StorageError::DownloadWorkflowConflict)?;
        Ok(report)
    }

    pub fn commit_legacy_download_cutover(
        &self,
        source_generation: u64,
        committed_at_ms: i64,
    ) -> Result<DownloadWorkflowAuthorityState, StorageError> {
        if source_generation == 0 || committed_at_ms < 0 {
            return Err(StorageError::DownloadWorkflowConflict);
        }
        let _ = self.recover_download_artifacts()?;
        self.write(|transaction| match read_authority(transaction)? {
            DownloadWorkflowAuthorityState::Staged {
                source_generation: staged,
            } if staged == source_generation => {
                let invalid: bool = transaction
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM pod0_download_workflows \
                         WHERE stage NOT IN('succeeded','requested'))",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|error| {
                        StorageError::sqlite("verify staged download cutover", error)
                    })?;
                if invalid {
                    return Err(StorageError::DownloadWorkflowConflict);
                }
                transaction
                    .execute(
                        "UPDATE pod0_episodes SET download_code=1,download_wire_code=NULL,\
                         download_ref_version=NULL,download_ref_key=NULL,download_byte_count=NULL \
                         WHERE episode_id IN(SELECT episode_id FROM pod0_download_workflows \
                         WHERE stage='requested')",
                        [],
                    )
                    .map_err(|error| {
                        StorageError::sqlite("clear restarted legacy downloads", error)
                    })?;
                transaction
                    .execute(
                        "UPDATE pod0_domain_cutovers SET state='authoritative',committed_at_ms=?1 \
                         WHERE domain=?2 AND state='staged' AND source_generation=?3",
                        params![
                            committed_at_ms,
                            CUTOVER_DOMAIN,
                            u64_to_i64(source_generation)?
                        ],
                    )
                    .map_err(|error| StorageError::sqlite("commit download cutover", error))?;
                if transaction.changes() != 1 {
                    return Err(StorageError::DownloadWorkflowConflict);
                }
                Ok(DownloadWorkflowAuthorityState::Authoritative { source_generation })
            }
            DownloadWorkflowAuthorityState::Authoritative {
                source_generation: current,
            } if current == source_generation => {
                Ok(DownloadWorkflowAuthorityState::Authoritative { source_generation })
            }
            _ => Err(StorageError::DownloadWorkflowConflict),
        })
    }

    pub fn download_cutover_report(&self) -> Result<LegacyDownloadCutoverReport, StorageError> {
        self.read(|connection| report(connection, read_authority(connection)?))
    }

    pub fn require_download_workflow_authoritative(&self) -> Result<(), StorageError> {
        if self.download_workflow_authority()?.is_authoritative() {
            Ok(())
        } else {
            Err(StorageError::CutoverNotAuthoritative)
        }
    }
}

pub(crate) fn validate_input(input: &LegacyDownloadCutoverInput) -> Result<(), StorageError> {
    if input.source_generation == 0
        || input.now_ms < 0
        || input.deadline_at_ms < input.now_ms
        || input.entries.len() > MAX_CUTOVER_ENTRIES
    {
        return Err(StorageError::DownloadWorkflowConflict);
    }
    let mut episodes = BTreeSet::new();
    let mut intents = BTreeSet::new();
    for entry in &input.entries {
        let input_valid = entry.input_version.len() == 64
            && entry
                .input_version
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit());
        let disposition_valid = match &entry.disposition {
            LegacyDownloadCutoverDisposition::Available {
                source_path,
                byte_count,
            } => !source_path.is_empty() && *byte_count > 0,
            LegacyDownloadCutoverDisposition::Restart { .. } => true,
        };
        if !input_valid
            || entry.enclosure_url.is_empty()
            || !episodes.insert(entry.episode_id)
            || !intents.insert(entry.intent_id)
            || !disposition_valid
        {
            return Err(StorageError::DownloadWorkflowConflict);
        }
    }
    Ok(())
}

pub(crate) fn read_authority(
    connection: &rusqlite::Connection,
) -> Result<DownloadWorkflowAuthorityState, StorageError> {
    let row: Option<(String, i64)> = connection
        .query_row(
            "SELECT state,source_generation FROM pod0_domain_cutovers WHERE domain=?1",
            [CUTOVER_DOMAIN],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read download workflow authority", error))?;
    match row {
        None => Ok(DownloadWorkflowAuthorityState::NotStarted),
        Some((state, generation)) => {
            let source_generation =
                u64::try_from(generation).map_err(|_| StorageError::DownloadWorkflowConflict)?;
            match state.as_str() {
                "staged" => Ok(DownloadWorkflowAuthorityState::Staged { source_generation }),
                "authoritative" => {
                    Ok(DownloadWorkflowAuthorityState::Authoritative { source_generation })
                }
                _ => Err(StorageError::DownloadWorkflowConflict),
            }
        }
    }
}

fn report(
    connection: &rusqlite::Connection,
    state: DownloadWorkflowAuthorityState,
) -> Result<LegacyDownloadCutoverReport, StorageError> {
    Ok(LegacyDownloadCutoverReport {
        state,
        adopted_available: count_stage(connection, StoredDownloadStage::Succeeded)?,
        scheduled_restart: count_stage(connection, StoredDownloadStage::Requested)?,
        repaired_invalid: 0,
    })
}

fn count_stage(
    connection: &rusqlite::Connection,
    stage: StoredDownloadStage,
) -> Result<u32, StorageError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pod0_download_workflows WHERE stage=?1",
            [stage.wire()],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("count migrated download workflows", error))?;
    u32::try_from(count).map_err(|_| StorageError::DownloadWorkflowConflict)
}
