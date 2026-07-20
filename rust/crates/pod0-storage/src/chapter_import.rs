use std::path::Path;

use pod0_domain::CommandId;

use crate::chapter_import_commit::commit_chapter_import;
use crate::chapter_import_discard::discard_chapter_import;
use crate::chapter_import_store_write::write_chapter_import;
use crate::chapter_import_verification::verify_chapter_import;
use crate::chapter_legacy_backup::create_or_reuse_chapter_backups;
use crate::legacy_chapter_source::inspect_chapter_source;
use crate::{
    CURRENT_SCHEMA_VERSION, ChapterImportPlan, ChapterImportReport, ChapterImportVerification,
    CoreStoreMigrator, MigrationClock, StorageError,
};

pub trait ChapterImportClock {
    fn now_milliseconds(&self) -> i64;
}

pub struct ChapterImporter<C> {
    clock: C,
}

impl<C: ChapterImportClock> ChapterImporter<C> {
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stage(
        &self,
        source_database_path: &Path,
        artifact_root: &Path,
        legacy_backup_root: &Path,
        target_path: &Path,
        target_schema_backup_path: &Path,
        expected_plan: &ChapterImportPlan,
        import_id: CommandId,
        target_store_id: CommandId,
    ) -> Result<ChapterImportReport, StorageError> {
        self.stage_with_observer(
            source_database_path,
            artifact_root,
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
        artifact_root: &Path,
        legacy_backup_root: &Path,
        target_path: &Path,
        target_schema_backup_path: &Path,
        expected_plan: &ChapterImportPlan,
        import_id: CommandId,
        target_store_id: CommandId,
        before_commit: F,
    ) -> Result<ChapterImportReport, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        let inspected = inspect_chapter_source(source_database_path, artifact_root)?;
        if inspected.plan != *expected_plan {
            return Err(StorageError::SourceChanged);
        }
        CoreStoreMigrator::new(ClockRef(&self.clock)).migrate(
            target_path,
            CURRENT_SCHEMA_VERSION,
            target_schema_backup_path,
            target_store_id,
        )?;
        let backup =
            create_or_reuse_chapter_backups(source_database_path, legacy_backup_root, &inspected)?;
        let current = inspect_chapter_source(source_database_path, artifact_root)?;
        if current != inspected {
            return Err(StorageError::SourceChanged);
        }
        write_chapter_import(
            target_path,
            import_id,
            target_store_id,
            &current,
            &backup,
            self.clock.now_milliseconds(),
            || {
                before_commit()?;
                if inspect_chapter_source(source_database_path, artifact_root)? != current {
                    return Err(StorageError::SourceChanged);
                }
                Ok(())
            },
        )
    }

    pub fn verify(
        &self,
        source_database_path: &Path,
        artifact_root: &Path,
        legacy_backup_root: &Path,
        target_path: &Path,
        import_id: CommandId,
    ) -> Result<ChapterImportVerification, StorageError> {
        verify_chapter_import(
            source_database_path,
            artifact_root,
            legacy_backup_root,
            target_path,
            import_id,
            self.clock.now_milliseconds(),
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn verify_with_observer<F>(
        &self,
        source_database_path: &Path,
        artifact_root: &Path,
        legacy_backup_root: &Path,
        target_path: &Path,
        import_id: CommandId,
        before_commit: F,
    ) -> Result<ChapterImportVerification, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        crate::chapter_import_verification::verify_chapter_import_with_observer(
            source_database_path,
            artifact_root,
            legacy_backup_root,
            target_path,
            import_id,
            self.clock.now_milliseconds(),
            before_commit,
        )
    }

    pub fn commit(
        &self,
        source_database_path: &Path,
        artifact_root: &Path,
        target_path: &Path,
        import_id: CommandId,
    ) -> Result<ChapterImportReport, StorageError> {
        commit_chapter_import(
            source_database_path,
            artifact_root,
            target_path,
            import_id,
            self.clock.now_milliseconds(),
        )
    }

    pub fn discard(
        &self,
        target_path: &Path,
        import_id: CommandId,
    ) -> Result<ChapterImportReport, StorageError> {
        discard_chapter_import(target_path, import_id, self.clock.now_milliseconds())
    }
}

struct ClockRef<'a, C>(&'a C);

impl<C: ChapterImportClock> MigrationClock for ClockRef<'_, C> {
    fn now_milliseconds(&self) -> i64 {
        self.0.now_milliseconds()
    }
}
