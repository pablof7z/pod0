//! The authoritative core store a scenario's facade opens.
//!
//! Pod0 has no first-class "fresh install" constructor yet: the production
//! Swift bootstrap (`App/Sources/Core/SharedLibraryBootstrap.swift`) prepares
//! the shared store by running the legacy import chain — listening, notes,
//! clips, transcripts, chapters, downloads, transcript workflows — against
//! whatever legacy sources exist, empty ones included, and only then calls
//! `Pod0Facade::open`. This module performs the SAME ritual through the SAME
//! public facade exports, with empty legacy sources, so every scenario starts
//! from the exact store shape a clean install produces. Nothing here reaches
//! into crate internals; if pod0 ever grows a first-class fresh-install
//! bootstrap, this module shrinks to one call.
//!
//! Timestamps are fixed constants, not wall-clock reads: the store must be
//! byte-stable across runs so a failing scenario reproduces exactly.

use std::path::{Path, PathBuf};

use pod0_domain::{CancellationId, CommandId, ContentDigest, StateRevision};
use pod0_facade::{
    LegacyChapterMigrationStage, commit_staged_legacy_chapter_import,
    commit_staged_legacy_clip_import, commit_staged_legacy_listening_import,
    commit_staged_legacy_note_import, commit_staged_legacy_transcript_import,
    inspect_legacy_chapter_migration, inspect_legacy_clip_source, inspect_legacy_listening_source,
    inspect_legacy_note_source, inspect_legacy_transcript_source, stage_legacy_chapter_import,
    stage_legacy_clip_import, stage_legacy_listening_import, stage_legacy_note_import,
    stage_legacy_transcript_import, verify_staged_legacy_chapter_import,
    verify_staged_legacy_transcript_import,
};

/// The fixture epoch every store-preparation step stamps. Scenario-time
/// observations start later (see `PodWorld::next_timestamp`), so prepared
/// history can never be mistaken for something a scenario did.
const PREPARED_AT: i64 = 1_800_000_000_000;

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Run the fresh-install preparation ritual inside `directory` and return the
/// store path `Pod0Facade::open` accepts. Panics loudly on any failure: a
/// scenario cannot mean anything against a half-prepared store.
pub(super) fn prepare_authoritative_store(directory: &tempfile::TempDir) -> PathBuf {
    let root = directory.path();
    let target = root.join("core.sqlite");
    let schema_backup = root.join("core.backup.sqlite");
    let legacy_json = root.join("legacy-listening.json");
    std::fs::write(&legacy_json, pod0_bdd::EMPTY_LEGACY_LISTENING_JSON)
        .expect("pod0-bdd: the empty legacy listening fixture must be writable");

    import_listening(&legacy_json, &target, &schema_backup, root);
    import_notes(&legacy_json, &target, &schema_backup, root);
    import_clips(&legacy_json, &target, &schema_backup, root);
    import_transcripts(&target, &schema_backup, root);
    import_chapters(&target, &schema_backup, root);
    cut_over_downloads(&target);
    cut_over_transcript_workflows(&target);
    target
}

fn import_listening(source: &Path, target: &Path, schema_backup: &Path, root: &Path) {
    let plan = inspect_legacy_listening_source(path_string(source))
        .expect("pod0-bdd: the empty legacy listening source must inspect cleanly");
    stage_legacy_listening_import(
        path_string(source),
        path_string(&root.join("legacy-listening.backup.json")),
        path_string(target),
        path_string(schema_backup),
        plan,
        CommandId::from_parts(0xBDD, 1),
        CommandId::from_parts(0xBDD, 2),
        PREPARED_AT,
    )
    .expect("pod0-bdd: staging the empty listening import must succeed");
    commit_staged_legacy_listening_import(path_string(target), PREPARED_AT + 1)
        .expect("pod0-bdd: committing the empty listening import must succeed");
}

fn import_notes(source: &Path, target: &Path, schema_backup: &Path, root: &Path) {
    let plan = inspect_legacy_note_source(path_string(source))
        .expect("pod0-bdd: the empty legacy note source must inspect cleanly");
    stage_legacy_note_import(
        path_string(source),
        path_string(&root.join("legacy-notes.backup.json")),
        path_string(target),
        path_string(schema_backup),
        plan,
        CommandId::from_parts(0xBDD, 3),
        CommandId::from_parts(0xBDD, 2),
        PREPARED_AT + 2,
    )
    .expect("pod0-bdd: staging the empty note import must succeed");
    commit_staged_legacy_note_import(path_string(target), PREPARED_AT + 3)
        .expect("pod0-bdd: committing the empty note import must succeed");
}

fn import_clips(source: &Path, target: &Path, schema_backup: &Path, root: &Path) {
    let plan = inspect_legacy_clip_source(path_string(source))
        .expect("pod0-bdd: the empty legacy clip source must inspect cleanly");
    stage_legacy_clip_import(
        path_string(source),
        path_string(&root.join("legacy-clips.backup.json")),
        path_string(target),
        path_string(schema_backup),
        plan,
        CommandId::from_parts(0xBDD, 4),
        CommandId::from_parts(0xBDD, 2),
        PREPARED_AT + 4,
    )
    .expect("pod0-bdd: staging the empty clip import must succeed");
    commit_staged_legacy_clip_import(path_string(source), path_string(target), PREPARED_AT + 5)
        .expect("pod0-bdd: committing the empty clip import must succeed");
}

/// The artifact-database schema both the legacy transcript and legacy chapter
/// importers expect their sqlite source to carry, exactly as the facade's own
/// fixtures state it.
const LEGACY_ARTIFACT_TABLES: &str = "CREATE TABLE artifacts(\
     id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,subject_id TEXT NOT NULL,\
     input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,\
     location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,\
     verified_at REAL NOT NULL,selected INTEGER NOT NULL,\
     UNIQUE(kind,subject_id,input_version,output_version));\
     CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);\
     INSERT INTO workflow_schema_versions VALUES('artifacts',1);";

fn import_transcripts(target: &Path, schema_backup: &Path, root: &Path) {
    let source = root.join("legacy-transcripts.sqlite");
    rusqlite::Connection::open(&source)
        .expect("pod0-bdd: the empty legacy transcript database must open")
        .execute_batch(LEGACY_ARTIFACT_TABLES)
        .expect("pod0-bdd: the empty legacy transcript schema must apply");
    let artifact_root = root.join("legacy-transcript-artifacts");
    let backup_root = root.join("legacy-transcript-backups");
    std::fs::create_dir_all(&artifact_root)
        .expect("pod0-bdd: the transcript artifact root must be creatable");
    let plan = inspect_legacy_transcript_source(path_string(&source), path_string(&artifact_root))
        .expect("pod0-bdd: the empty legacy transcript source must inspect cleanly");
    let import_id = CommandId::from_parts(0xBDD, 5);
    stage_legacy_transcript_import(
        path_string(&source),
        path_string(&artifact_root),
        path_string(&backup_root),
        path_string(target),
        path_string(schema_backup),
        plan,
        import_id,
        CommandId::from_parts(0xBDD, 2),
        PREPARED_AT + 6,
    )
    .expect("pod0-bdd: staging the empty transcript import must succeed");
    verify_staged_legacy_transcript_import(
        path_string(target),
        path_string(&backup_root),
        import_id,
        PREPARED_AT + 7,
    )
    .expect("pod0-bdd: verifying the empty transcript import must succeed");
    commit_staged_legacy_transcript_import(
        path_string(&source),
        path_string(&artifact_root),
        path_string(target),
        import_id,
        PREPARED_AT + 8,
    )
    .expect("pod0-bdd: committing the empty transcript import must succeed");
}

fn import_chapters(target: &Path, schema_backup: &Path, root: &Path) {
    let source = root.join("legacy-chapters.sqlite");
    rusqlite::Connection::open(&source)
        .expect("pod0-bdd: the empty legacy chapter database must open")
        .execute_batch(
            "CREATE TABLE episodes(id TEXT PRIMARY KEY,subscription_id TEXT NOT NULL,\
             guid TEXT NOT NULL,pub_date REAL NOT NULL,sort_order INTEGER NOT NULL,\
             payload BLOB NOT NULL);\
             CREATE TABLE persistence_metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL);\
             INSERT INTO persistence_metadata VALUES('generation','1');",
        )
        .expect("pod0-bdd: the empty legacy chapter episode schema must apply");
    rusqlite::Connection::open(&source)
        .expect("pod0-bdd: the empty legacy chapter database must reopen")
        .execute_batch(LEGACY_ARTIFACT_TABLES)
        .expect("pod0-bdd: the empty legacy chapter artifact schema must apply");
    let artifact_root = root.join("legacy-chapter-artifacts");
    let backup_root = root.join("legacy-chapter-backup");
    std::fs::create_dir_all(&artifact_root)
        .expect("pod0-bdd: the chapter artifact root must be creatable");
    let plan = inspect_legacy_chapter_migration(path_string(&source), path_string(&artifact_root))
        .plan
        .expect("pod0-bdd: the empty legacy chapter source must yield a plan");
    let import_id = CommandId::from_parts(0xBDD, 6);
    let staged = stage_legacy_chapter_import(
        path_string(&source),
        path_string(&artifact_root),
        path_string(&backup_root),
        path_string(target),
        path_string(schema_backup),
        plan,
        import_id,
        CommandId::from_parts(0xBDD, 2),
    );
    assert!(
        matches!(staged.stage, LegacyChapterMigrationStage::Staged),
        "pod0-bdd: staging the empty chapter import must succeed, got {staged:?}"
    );
    let verified = verify_staged_legacy_chapter_import(
        path_string(&source),
        path_string(&artifact_root),
        path_string(&backup_root),
        path_string(target),
        import_id,
    );
    assert!(
        matches!(verified.stage, LegacyChapterMigrationStage::Verified),
        "pod0-bdd: verifying the empty chapter import must succeed, got {verified:?}"
    );
    let imported = commit_staged_legacy_chapter_import(
        path_string(&source),
        path_string(&artifact_root),
        path_string(target),
        import_id,
    );
    assert!(
        matches!(imported.stage, LegacyChapterMigrationStage::Imported),
        "pod0-bdd: committing the empty chapter import must succeed, got {imported:?}"
    );
}

fn cut_over_downloads(target: &Path) {
    let store = pod0_storage::LibraryStore::open_authoritative(target)
        .expect("pod0-bdd: the prepared store must open for the download cutover");
    store
        .stage_legacy_download_cutover(pod0_storage::LegacyDownloadCutoverInput {
            source_generation: 1,
            entries: Vec::new(),
            issued_revision: StateRevision::INITIAL,
            now_ms: PREPARED_AT + 9,
            deadline_at_ms: PREPARED_AT + 60_009,
        })
        .expect("pod0-bdd: staging the empty download cutover must succeed");
    store
        .commit_legacy_download_cutover(1, PREPARED_AT + 10)
        .expect("pod0-bdd: committing the empty download cutover must succeed");
}

fn cut_over_transcript_workflows(target: &Path) {
    let store = pod0_storage::LibraryStore::open_authoritative(target)
        .expect("pod0-bdd: the prepared store must open for the transcript workflow cutover");
    let fingerprint = pod0_storage::transcript_workflow_source_fingerprint(&[]);
    store
        .stage_legacy_transcript_workflow_cutover(
            pod0_storage::LegacyTranscriptWorkflowCutoverInput {
                source_generation: 1,
                source_fingerprint: fingerprint,
                backup_digest: ContentDigest::default(),
                backup_byte_count: 0,
                rows: Vec::new(),
                candidates: Vec::new(),
                command_id: CommandId::from_parts(0xBDD, 7),
                cancellation_id: CancellationId::from_parts(0xBDD, 8),
                issued_revision: StateRevision::INITIAL,
                max_attempts: pod0_application::TRANSCRIPT_WORKFLOW_MAX_ATTEMPTS,
                now_ms: PREPARED_AT + 11,
            },
        )
        .expect("pod0-bdd: staging the empty transcript workflow cutover must succeed");
    store
        .verify_legacy_transcript_workflow_cutover(1, fingerprint, PREPARED_AT + 12)
        .expect("pod0-bdd: verifying the empty transcript workflow cutover must succeed");
    store
        .commit_legacy_transcript_workflow_cutover(1, fingerprint, PREPARED_AT + 13)
        .expect("pod0-bdd: committing the empty transcript workflow cutover must succeed");
}
