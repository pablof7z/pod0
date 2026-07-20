use super::*;

pub(super) fn install_chapter_fixture(directory: &tempfile::TempDir, target: &std::path::Path) {
    let source = directory.path().join("legacy-chapters.sqlite");
    let artifact_root = directory.path().join("legacy-chapter-artifacts");
    let backup_root = directory.path().join("legacy-chapter-backup");
    let schema_backup = directory.path().join("chapter-schema-backup.sqlite");
    std::fs::create_dir_all(&artifact_root).unwrap();
    let connection = Connection::open(&source).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE episodes(id TEXT PRIMARY KEY,subscription_id TEXT NOT NULL,\
               guid TEXT NOT NULL,pub_date REAL NOT NULL,sort_order INTEGER NOT NULL,\
               payload BLOB NOT NULL);\
             CREATE TABLE persistence_metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL);\
             INSERT INTO persistence_metadata VALUES('generation','7');\
             CREATE TABLE artifacts(\
               id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,subject_id TEXT NOT NULL,\
               input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,\
               location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,\
               verified_at REAL NOT NULL,selected INTEGER NOT NULL,\
               UNIQUE(kind,subject_id,input_version,output_version));\
             CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);\
             INSERT INTO workflow_schema_versions VALUES('artifacts',1);",
        )
        .unwrap();
    let payload = br#"{
      "id":"22222222-2222-2222-2222-222222222222",
      "podcastID":"11111111-1111-1111-1111-111111111111",
      "pubDate":"2024-01-02T00:00:00Z",
      "duration":120.5,
      "chapters":[
        {"id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa","startTime":0.0,
         "title":"Opening","includeInTableOfContents":true,"isAIGenerated":false},
        {"id":"bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb","startTime":60.0,
         "title":"Deep dive","includeInTableOfContents":true,"isAIGenerated":false}
      ],
      "adSegments":[
        {"id":"cccccccc-cccc-cccc-cccc-cccccccccccc","start":0.0,"end":10.0,
         "kind":"preroll"},
        {"id":"dddddddd-dddd-dddd-dddd-dddddddddddd","start":40.0,"end":50.0,
         "kind":"midroll"},
        {"id":"eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee","start":110.0,"end":120.0,
         "kind":"postroll"}
      ]
    }"#;
    connection
        .execute(
            "INSERT INTO episodes(id,subscription_id,guid,pub_date,sort_order,payload) \
             VALUES(?1,?2,'legacy-kotlin-guid',0,0,?3)",
            rusqlite::params![
                "22222222-2222-2222-2222-222222222222",
                "11111111-1111-1111-1111-111111111111",
                payload.as_slice(),
            ],
        )
        .unwrap();
    drop(connection);

    let inspected = inspect_legacy_chapter_migration(
        source.to_string_lossy().into_owned(),
        artifact_root.to_string_lossy().into_owned(),
    );
    let plan = inspected.plan.expect("chapter fixture plan");
    assert_eq!(plan.blocked_count, 0);
    let import_id = CommandId::from_parts(9, 8);
    let staged = stage_legacy_chapter_import(
        source.to_string_lossy().into_owned(),
        artifact_root.to_string_lossy().into_owned(),
        backup_root.to_string_lossy().into_owned(),
        target.to_string_lossy().into_owned(),
        schema_backup.to_string_lossy().into_owned(),
        plan,
        import_id,
        CommandId::from_parts(9, 2),
    );
    assert_eq!(
        staged.report.as_ref().map(|report| report.state),
        Some(LegacyChapterImportState::Staged)
    );
    let verified = verify_staged_legacy_chapter_import(
        source.to_string_lossy().into_owned(),
        artifact_root.to_string_lossy().into_owned(),
        backup_root.to_string_lossy().into_owned(),
        target.to_string_lossy().into_owned(),
        import_id,
    );
    assert_eq!(
        verified.report.as_ref().map(|report| report.state),
        Some(LegacyChapterImportState::Verified)
    );
    let imported = commit_staged_legacy_chapter_import(
        source.to_string_lossy().into_owned(),
        artifact_root.to_string_lossy().into_owned(),
        target.to_string_lossy().into_owned(),
        import_id,
    );
    assert_eq!(
        imported.report.as_ref().map(|report| report.state),
        Some(LegacyChapterImportState::Imported)
    );
}

pub(super) fn install_empty_chapter_fixture(
    directory: &tempfile::TempDir,
    target: &std::path::Path,
) {
    let source = directory.path().join("legacy-empty-chapters.sqlite");
    let artifact_root = directory.path().join("legacy-empty-chapter-artifacts");
    let backup_root = directory.path().join("legacy-empty-chapter-backup");
    let schema_backup = directory.path().join("empty-chapter-schema-backup.sqlite");
    std::fs::create_dir_all(&artifact_root).unwrap();
    Connection::open(&source)
        .unwrap()
        .execute_batch(
            "CREATE TABLE episodes(id TEXT PRIMARY KEY,subscription_id TEXT NOT NULL,\
             guid TEXT NOT NULL,pub_date REAL NOT NULL,sort_order INTEGER NOT NULL,\
             payload BLOB NOT NULL);\
             CREATE TABLE persistence_metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL);\
             INSERT INTO persistence_metadata VALUES('generation','7');\
             CREATE TABLE artifacts(\
             id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,subject_id TEXT NOT NULL,\
             input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,\
             location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,\
             verified_at REAL NOT NULL,selected INTEGER NOT NULL,\
             UNIQUE(kind,subject_id,input_version,output_version));\
             CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);\
             INSERT INTO workflow_schema_versions VALUES('artifacts',1);",
        )
        .unwrap();
    let plan = inspect_legacy_chapter_migration(
        source.to_string_lossy().into_owned(),
        artifact_root.to_string_lossy().into_owned(),
    )
    .plan
    .unwrap();
    let import_id = CommandId::from_parts(9, 18);
    assert!(matches!(
        stage_legacy_chapter_import(
            source.to_string_lossy().into_owned(),
            artifact_root.to_string_lossy().into_owned(),
            backup_root.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
            schema_backup.to_string_lossy().into_owned(),
            plan,
            import_id,
            CommandId::from_parts(9, 2),
        )
        .stage,
        LegacyChapterMigrationStage::Staged
    ));
    assert!(matches!(
        verify_staged_legacy_chapter_import(
            source.to_string_lossy().into_owned(),
            artifact_root.to_string_lossy().into_owned(),
            backup_root.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
            import_id,
        )
        .stage,
        LegacyChapterMigrationStage::Verified
    ));
    assert!(matches!(
        commit_staged_legacy_chapter_import(
            source.to_string_lossy().into_owned(),
            artifact_root.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
            import_id,
        )
        .stage,
        LegacyChapterMigrationStage::Imported
    ));
}
