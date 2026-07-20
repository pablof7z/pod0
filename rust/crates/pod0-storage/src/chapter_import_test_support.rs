use std::fs;
use std::path::PathBuf;

use pod0_domain::CommandId;
use rusqlite::{Connection, params};

use crate::transcript_import_digest::{digest_bytes, hex_digest};
use crate::{
    ChapterImportClock, ChapterImportPlan, ChapterImportReport, ChapterImportVerification,
    ChapterImporter, LegacyChapterSourceKind, inspect_legacy_chapter_source,
};

pub(crate) const EPISODE_ID: &str = "11111111-1111-1111-1111-111111111111";
pub(crate) const SECOND_EPISODE_ID: &str = "33333333-3333-3333-3333-333333333333";
pub(crate) const PODCAST_ID: &str = "22222222-2222-2222-2222-222222222222";
pub(crate) const IMPORT_ID: CommandId = CommandId::from_parts(9, 9);
pub(crate) const STORE_ID: CommandId = CommandId::from_parts(8, 8);

#[derive(Clone, Copy)]
pub(crate) struct FixedClock(pub(crate) i64);

impl ChapterImportClock for FixedClock {
    fn now_milliseconds(&self) -> i64 {
        self.0
    }
}

pub(crate) struct ChapterImportFixture {
    pub(crate) _directory: tempfile::TempDir,
    pub(crate) source: PathBuf,
    pub(crate) artifacts: PathBuf,
    pub(crate) legacy_backup: PathBuf,
    pub(crate) target: PathBuf,
    pub(crate) schema_backup: PathBuf,
    pub(crate) rollback_root: PathBuf,
    source_kind: LegacyChapterSourceKind,
}

impl ChapterImportFixture {
    pub(crate) fn new_v0() -> Self {
        Self::new(LegacyChapterSourceKind::ArtifactSqliteV0)
    }

    pub(crate) fn new_v1() -> Self {
        Self::new(LegacyChapterSourceKind::ArtifactSqliteV1)
    }

    fn new(source_kind: LegacyChapterSourceKind) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("swift.sqlite");
        let artifacts = directory.path().join("workflow-artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        let connection = Connection::open(&source).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE episodes(id TEXT PRIMARY KEY,subscription_id TEXT NOT NULL,
                   guid TEXT NOT NULL,pub_date REAL NOT NULL,sort_order INTEGER NOT NULL,
                   payload BLOB NOT NULL);
                 CREATE TABLE persistence_metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL);
                 INSERT INTO persistence_metadata VALUES('generation','7');",
            )
            .unwrap();
        create_artifact_schema(&connection, source_kind);
        drop(connection);
        Self {
            legacy_backup: directory.path().join("legacy-backup"),
            target: directory.path().join("core.sqlite"),
            schema_backup: directory.path().join("schema-backup.sqlite"),
            rollback_root: directory.path().join("rollback"),
            _directory: directory,
            source,
            artifacts,
            source_kind,
        }
    }

    pub(crate) fn insert_episode(&self, id: &str, podcast: &str, payload: &str) {
        let connection = Connection::open(&self.source).unwrap();
        connection
            .execute(
                "INSERT INTO episodes(id,subscription_id,guid,pub_date,sort_order,payload) \
                 VALUES(?1,?2,'guid',0,0,?3)",
                params![id, podcast, payload.as_bytes()],
            )
            .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_workflow_artifact(
        &self,
        kind: &str,
        subject: &str,
        input_version: &str,
        output_version: &str,
        origin: &str,
        integrity: &str,
        verified_at: f64,
        selected: bool,
        payload: &str,
    ) -> PathBuf {
        let digest = digest_bytes(payload.as_bytes());
        let directory = if kind == "chapters" {
            "chapters"
        } else {
            "ads"
        };
        let path = self
            .artifacts
            .join(directory)
            .join(subject)
            .join(format!("{}.json", hex_digest(digest)));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, payload).unwrap();
        let connection = Connection::open(&self.source).unwrap();
        match self.source_kind {
            LegacyChapterSourceKind::ArtifactSqliteV0 => connection
                .execute(
                    "INSERT INTO artifacts(kind,subject_id,input_version,output_version,
                     content_hash,location,origin,schema_version,integrity,verified_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8,?9)",
                    params![
                        kind,
                        subject,
                        input_version,
                        output_version,
                        hex_digest(digest),
                        path.to_str(),
                        origin,
                        integrity,
                        verified_at,
                    ],
                )
                .unwrap(),
            LegacyChapterSourceKind::ArtifactSqliteV1 => connection
                .execute(
                    "INSERT INTO artifacts(kind,subject_id,input_version,output_version,
                     content_hash,location,origin,schema_version,integrity,verified_at,selected)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,1,?8,?9,?10)",
                    params![
                        kind,
                        subject,
                        input_version,
                        output_version,
                        hex_digest(digest),
                        path.to_str(),
                        origin,
                        integrity,
                        verified_at,
                        selected,
                    ],
                )
                .unwrap(),
        };
        path
    }

    pub(crate) fn inspect(&self) -> ChapterImportPlan {
        inspect_legacy_chapter_source(&self.source, &self.artifacts).unwrap()
    }

    pub(crate) fn stage(&self, clock: i64) -> ChapterImportReport {
        let plan = self.inspect();
        ChapterImporter::new(FixedClock(clock))
            .stage(
                &self.source,
                &self.artifacts,
                &self.legacy_backup,
                &self.target,
                &self.schema_backup,
                &plan,
                IMPORT_ID,
                STORE_ID,
            )
            .unwrap()
    }

    pub(crate) fn verify(&self, clock: i64) -> ChapterImportVerification {
        ChapterImporter::new(FixedClock(clock))
            .verify(
                &self.source,
                &self.artifacts,
                &self.legacy_backup,
                &self.target,
                IMPORT_ID,
            )
            .unwrap()
    }

    pub(crate) fn import(&self, clock: i64) -> ChapterImportReport {
        ChapterImporter::new(FixedClock(clock))
            .commit(&self.source, &self.artifacts, &self.target, IMPORT_ID)
            .unwrap()
    }

    pub(crate) fn target_connection(&self) -> Connection {
        Connection::open(&self.target).unwrap()
    }
}

fn create_artifact_schema(connection: &Connection, kind: LegacyChapterSourceKind) {
    let selected = match kind {
        LegacyChapterSourceKind::ArtifactSqliteV0 => "",
        LegacyChapterSourceKind::ArtifactSqliteV1 => ",selected INTEGER NOT NULL",
    };
    connection
        .execute_batch(&format!(
            "CREATE TABLE artifacts(
               id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,subject_id TEXT NOT NULL,
               input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,
               location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,
               verified_at REAL NOT NULL{selected},
               UNIQUE(kind,subject_id,input_version,output_version));
             CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);
             INSERT INTO workflow_schema_versions VALUES('artifacts',{});",
            kind.schema_version()
        ))
        .unwrap();
}
