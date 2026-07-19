use std::path::Path;

use pod0_domain::CommandId;

use crate::clip_import_store::write_clip_import;
use crate::clip_import_store_support::source_still_matches;
use crate::clip_legacy_backup::create_or_reuse_clip_backup;
use crate::{
    CURRENT_SCHEMA_VERSION, ClipImportPlan, ClipImportReport, CoreStoreMigrator, MigrationClock,
    StorageError,
};

pub trait ClipImportClock {
    fn now_milliseconds(&self) -> i64;
}

pub struct ClipImporter<C> {
    clock: C,
}

impl<C: ClipImportClock> ClipImporter<C> {
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn stage(
        &self,
        source_path: &Path,
        source_backup_path: &Path,
        target_path: &Path,
        target_schema_backup_path: &Path,
        expected_plan: &ClipImportPlan,
        import_id: CommandId,
        target_store_id: CommandId,
    ) -> Result<ClipImportReport, StorageError> {
        self.stage_with_observer(
            source_path,
            source_backup_path,
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
        source_path: &Path,
        source_backup_path: &Path,
        target_path: &Path,
        target_schema_backup_path: &Path,
        expected_plan: &ClipImportPlan,
        import_id: CommandId,
        target_store_id: CommandId,
        before_commit: F,
    ) -> Result<ClipImportReport, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        let inspected = source_still_matches(source_path, expected_plan)?;
        let backup = create_or_reuse_clip_backup(source_path, source_backup_path, expected_plan)?;
        CoreStoreMigrator::new(ClockRef(&self.clock)).migrate(
            target_path,
            CURRENT_SCHEMA_VERSION,
            target_schema_backup_path,
            target_store_id,
        )?;
        let current = source_still_matches(source_path, expected_plan)?;
        if current.clips != inspected.clips {
            return Err(StorageError::SourceChanged);
        }
        write_clip_import(
            target_path,
            import_id,
            &current,
            &backup,
            self.clock.now_milliseconds(),
            before_commit,
        )
    }
}

struct ClockRef<'a, C>(&'a C);

impl<C: ClipImportClock> MigrationClock for ClockRef<'_, C> {
    fn now_milliseconds(&self) -> i64 {
        self.0.now_milliseconds()
    }
}
