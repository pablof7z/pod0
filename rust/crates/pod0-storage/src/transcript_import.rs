use std::path::Path;

use pod0_domain::CommandId;

use crate::legacy_transcript_source::inspect_transcript_source;
use crate::transcript_import_commit::commit_transcript_import;
use crate::transcript_import_discard::discard_transcript_import;
use crate::transcript_import_store_write::write_transcript_import;
use crate::transcript_import_verification::verify_transcript_import;
use crate::transcript_legacy_backup::create_or_reuse_transcript_backups;
use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, MigrationClock, StorageError, TranscriptImportPlan,
    TranscriptImportReport, TranscriptImportVerification,
};

pub trait TranscriptImportClock {
    fn now_milliseconds(&self) -> i64;
}

pub struct TranscriptImporter<C> {
    clock: C,
}

impl<C: TranscriptImportClock> TranscriptImporter<C> {
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stage(
        &self,
        source_database_path: &Path,
        transcript_root: &Path,
        legacy_backup_root: &Path,
        target_path: &Path,
        target_schema_backup_path: &Path,
        expected_plan: &TranscriptImportPlan,
        import_id: CommandId,
        target_store_id: CommandId,
    ) -> Result<TranscriptImportReport, StorageError> {
        self.stage_with_observer(
            source_database_path,
            transcript_root,
            legacy_backup_root,
            target_path,
            target_schema_backup_path,
            expected_plan,
            import_id,
            target_store_id,
            || Ok(()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stage_with_observer<F>(
        &self,
        source_database_path: &Path,
        transcript_root: &Path,
        legacy_backup_root: &Path,
        target_path: &Path,
        target_schema_backup_path: &Path,
        expected_plan: &TranscriptImportPlan,
        import_id: CommandId,
        target_store_id: CommandId,
        before_commit: F,
    ) -> Result<TranscriptImportReport, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        let inspected = inspect_transcript_source(source_database_path, transcript_root)?;
        if inspected.plan != *expected_plan {
            return Err(StorageError::SourceChanged);
        }
        CoreStoreMigrator::new(ClockRef(&self.clock)).migrate(
            target_path,
            CURRENT_SCHEMA_VERSION,
            target_schema_backup_path,
            target_store_id,
        )?;
        if crate::transcript_store_is_authoritative(target_path)? {
            return Err(StorageError::CutoverAlreadyAuthoritative);
        }
        let backup = create_or_reuse_transcript_backups(
            source_database_path,
            legacy_backup_root,
            &inspected,
        )?;
        let current = inspect_transcript_source(source_database_path, transcript_root)?;
        if current != inspected {
            return Err(StorageError::SourceChanged);
        }
        write_transcript_import(
            target_path,
            import_id,
            &current,
            &backup,
            self.clock.now_milliseconds(),
            || {
                before_commit()?;
                if inspect_transcript_source(source_database_path, transcript_root)? != current {
                    return Err(StorageError::SourceChanged);
                }
                Ok(())
            },
        )
    }

    pub fn verify(
        &self,
        target_path: &Path,
        legacy_backup_root: &Path,
        import_id: CommandId,
    ) -> Result<TranscriptImportVerification, StorageError> {
        verify_transcript_import(
            target_path,
            legacy_backup_root,
            import_id,
            self.clock.now_milliseconds(),
        )
    }

    pub fn commit(
        &self,
        source_database_path: &Path,
        transcript_root: &Path,
        target_path: &Path,
        import_id: CommandId,
    ) -> Result<TranscriptImportReport, StorageError> {
        commit_transcript_import(
            source_database_path,
            transcript_root,
            target_path,
            import_id,
            self.clock.now_milliseconds(),
        )
    }

    pub fn discard(
        &self,
        target_path: &Path,
        import_id: CommandId,
    ) -> Result<TranscriptImportReport, StorageError> {
        discard_transcript_import(target_path, import_id, self.clock.now_milliseconds())
    }
}

struct ClockRef<'a, C>(&'a C);

impl<C: TranscriptImportClock> MigrationClock for ClockRef<'_, C> {
    fn now_milliseconds(&self) -> i64 {
        self.0.now_milliseconds()
    }
}
