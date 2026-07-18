use std::path::Path;

use pod0_domain::CommandId;

use crate::legacy_backup::create_or_reuse_legacy_backup;
use crate::legacy_source::inspect_source;
use crate::listening_store_write::write_import;
use crate::{
    CURRENT_SCHEMA_VERSION, CoreStoreMigrator, LegacyImportPlan, ListeningImportReport,
    MigrationClock, StorageError,
};

pub trait ListeningImportClock {
    fn now_milliseconds(&self) -> i64;
}

pub struct ListeningImporter<C> {
    clock: C,
}

impl<C: ListeningImportClock> ListeningImporter<C> {
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
        expected_plan: &LegacyImportPlan,
        import_id: CommandId,
        target_store_id: CommandId,
    ) -> Result<ListeningImportReport, StorageError> {
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
        expected_plan: &LegacyImportPlan,
        import_id: CommandId,
        target_store_id: CommandId,
        before_commit: F,
    ) -> Result<ListeningImportReport, StorageError>
    where
        F: FnOnce() -> Result<(), StorageError>,
    {
        let inspected = inspect_source(source_path)?;
        if &inspected.plan != expected_plan {
            return Err(StorageError::SourceChanged);
        }
        let backup = create_or_reuse_legacy_backup(source_path, source_backup_path, expected_plan)?;
        CoreStoreMigrator::new(ClockRef(&self.clock)).migrate(
            target_path,
            CURRENT_SCHEMA_VERSION,
            target_schema_backup_path,
            target_store_id,
        )?;
        let current = inspect_source(source_path)?;
        if current.plan != inspected.plan || current.snapshot != inspected.snapshot {
            return Err(StorageError::SourceChanged);
        }
        write_import(
            target_path,
            target_store_id,
            import_id,
            &current,
            &backup,
            self.clock.now_milliseconds(),
            before_commit,
        )
    }
}

struct ClockRef<'a, C>(&'a C);

impl<C: ListeningImportClock> MigrationClock for ClockRef<'_, C> {
    fn now_milliseconds(&self) -> i64 {
        self.0.now_milliseconds()
    }
}
