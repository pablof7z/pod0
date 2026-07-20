use std::fs;

use rusqlite::Connection;

use crate::{
    CommandId, LegacyChapterImportState, LegacyChapterMigrationFailureCode,
    LegacyChapterMigrationStage, commit_staged_legacy_chapter_import,
    export_legacy_chapter_rollback, inspect_legacy_chapter_migration,
    read_active_legacy_chapter_migration, stage_legacy_chapter_import,
    verify_staged_legacy_chapter_import,
};

#[test]
fn state_shaped_facade_imports_empty_history_and_exports_rollback() {
    let fixture = Fixture::new();
    let inspected = inspect_legacy_chapter_migration(fixture.source(), fixture.artifacts());
    assert_eq!(inspected.stage, LegacyChapterMigrationStage::Inspected);
    let plan = inspected.plan.unwrap();
    assert_eq!((plan.evidence_count, plan.canonical_artifact_count), (0, 0));

    let staged = stage_legacy_chapter_import(
        fixture.source(),
        fixture.artifacts(),
        fixture.backups(),
        fixture.target(),
        fixture.schema_backup(),
        plan,
        CommandId::from_parts(1, 1),
        CommandId::from_parts(2, 2),
    );
    assert_eq!(staged.stage, LegacyChapterMigrationStage::Staged);
    assert_eq!(staged.failure, None);

    let verified = verify_staged_legacy_chapter_import(
        fixture.source(),
        fixture.artifacts(),
        fixture.backups(),
        fixture.target(),
        CommandId::from_parts(1, 1),
    );
    assert_eq!(verified.stage, LegacyChapterMigrationStage::Verified);
    assert_eq!(verified.verification.unwrap().verified_evidence_count, 0);

    let imported = commit_staged_legacy_chapter_import(
        fixture.source(),
        fixture.artifacts(),
        fixture.target(),
        CommandId::from_parts(1, 1),
    );
    assert_eq!(imported.stage, LegacyChapterMigrationStage::Imported);
    assert_eq!(
        imported.report.unwrap().state,
        LegacyChapterImportState::Imported
    );
    assert_eq!(
        read_active_legacy_chapter_migration(fixture.target()).stage,
        LegacyChapterMigrationStage::Imported
    );

    let rollback =
        export_legacy_chapter_rollback(fixture.target(), fixture.backups(), fixture.rollback());
    assert_eq!(rollback.stage, LegacyChapterMigrationStage::Imported);
    let report = rollback.rollback_export.unwrap();
    assert_eq!((report.format_version, report.evidence_count), (1, 0));
    assert!(std::path::Path::new(&report.bundle_path).is_dir());
}

#[test]
fn source_failures_are_bounded_state_not_ffi_errors() {
    let fixture = Fixture::new();
    Connection::open(fixture.source())
        .unwrap()
        .execute_batch(
            "CREATE TABLE artifacts(
               id INTEGER PRIMARY KEY,kind TEXT NOT NULL,subject_id TEXT NOT NULL,
               input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,
               location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,
               verified_at REAL NOT NULL,selected INTEGER NOT NULL);
             CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);
             INSERT INTO workflow_schema_versions VALUES('artifacts',2);",
        )
        .unwrap();

    let projection = inspect_legacy_chapter_migration(fixture.source(), fixture.artifacts());
    assert_eq!(projection.stage, LegacyChapterMigrationStage::Blocked);
    let failure = projection.failure.unwrap();
    assert_eq!(
        failure.code,
        LegacyChapterMigrationFailureCode::SourceInvalid
    );
    assert_eq!(failure.diagnostic_code, "newer_legacy_chapter_schema");
}

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("artifacts")).unwrap();
        Connection::open(directory.path().join("swift.sqlite"))
            .unwrap()
            .execute_batch(
                "CREATE TABLE episodes(id TEXT PRIMARY KEY,subscription_id TEXT NOT NULL,
                   payload BLOB NOT NULL);
                 CREATE TABLE persistence_metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL);
                 INSERT INTO persistence_metadata VALUES('generation','0');",
            )
            .unwrap();
        Self { directory }
    }

    fn source(&self) -> String {
        self.path("swift.sqlite")
    }

    fn artifacts(&self) -> String {
        self.path("artifacts")
    }

    fn backups(&self) -> String {
        self.path("backups")
    }

    fn target(&self) -> String {
        self.path("core.sqlite")
    }

    fn schema_backup(&self) -> String {
        self.path("schema-backup.sqlite")
    }

    fn rollback(&self) -> String {
        self.path("rollback")
    }

    fn path(&self, name: &str) -> String {
        self.directory
            .path()
            .join(name)
            .to_str()
            .unwrap()
            .to_owned()
    }
}
