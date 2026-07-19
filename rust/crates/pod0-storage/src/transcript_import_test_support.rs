use std::fs;
use std::path::{Path, PathBuf};

use pod0_domain::CommandId;
use rusqlite::{Connection, params};
use serde_json::json;

use crate::listening_import_test_support::{
    EPISODE_ID, ImportFixture, create_sqlite_source, current_metadata, episode,
};
use crate::transcript_import_digest::{digest_bytes, hex_digest};
use crate::{
    LegacyTranscriptSourceKind, StorageError, TranscriptImportClock, TranscriptImportPlan,
    TranscriptImportReport, TranscriptImportVerification, TranscriptImporter,
    inspect_legacy_transcript_source,
};

pub(crate) struct TranscriptImportFixture {
    pub(crate) import: ImportFixture,
    pub(crate) transcript_root: PathBuf,
    pub(crate) backup_root: PathBuf,
    pub(crate) selected_path: PathBuf,
    pub(crate) importer: TranscriptImporter<FixedTranscriptClock>,
}

impl TranscriptImportFixture {
    pub(crate) fn current() -> Self {
        Self::new(LegacyTranscriptSourceKind::ArtifactSqliteV1)
    }

    pub(crate) fn legacy_v0() -> Self {
        Self::new(LegacyTranscriptSourceKind::ArtifactSqliteV0)
    }

    fn new(kind: LegacyTranscriptSourceKind) -> Self {
        let import = ImportFixture::new();
        create_sqlite_source(
            &import.source,
            &current_metadata(12),
            &[episode(EPISODE_ID, "transcript-import-guid")],
        );
        let transcript_root = import._directory.path().join("transcripts");
        let backup_root = import._directory.path().join("transcript-backups");
        fs::create_dir_all(&transcript_root).unwrap();
        let selected_path = transcript_root.join("selected.json");
        let bytes = transcript_json(EPISODE_ID, "Small habits become durable");
        fs::write(&selected_path, &bytes).unwrap();
        create_artifact_schema(&import.source, kind, &selected_path, &bytes);
        import.stage(&import.plan()).unwrap();
        Self {
            import,
            transcript_root,
            backup_root,
            selected_path,
            importer: TranscriptImporter::new(FixedTranscriptClock),
        }
    }

    pub(crate) fn plan(&self) -> TranscriptImportPlan {
        inspect_legacy_transcript_source(&self.import.source, &self.transcript_root).unwrap()
    }

    pub(crate) fn stage(
        &self,
        import_id: CommandId,
    ) -> Result<TranscriptImportReport, StorageError> {
        self.importer.stage(
            &self.import.source,
            &self.transcript_root,
            &self.backup_root,
            &self.import.target,
            &self.import.target_backup,
            &self.plan(),
            import_id,
            command(900),
        )
    }

    pub(crate) fn verify(
        &self,
        import_id: CommandId,
    ) -> Result<TranscriptImportVerification, StorageError> {
        self.importer
            .verify(&self.import.target, &self.backup_root, import_id)
    }

    pub(crate) fn commit(
        &self,
        import_id: CommandId,
    ) -> Result<TranscriptImportReport, StorageError> {
        self.importer.commit(
            &self.import.source,
            &self.transcript_root,
            &self.import.target,
            import_id,
        )
    }

    pub(crate) fn replace_selected(&self, text: &str) {
        let bytes = transcript_json(EPISODE_ID, text);
        self.replace_selected_bytes(&bytes);
    }

    pub(crate) fn replace_selected_bytes(&self, bytes: &[u8]) {
        fs::write(&self.selected_path, bytes).unwrap();
        Connection::open(&self.import.source)
            .unwrap()
            .execute(
                "UPDATE artifacts SET content_hash=?1,output_version=?2,verified_at=verified_at+1 \
                 WHERE kind='transcript' AND selected=1",
                params![
                    hex_digest(digest_bytes(bytes)),
                    format!("output-{}", hex_digest(digest_bytes(bytes)))
                ],
            )
            .unwrap();
    }
}

pub(crate) struct FixedTranscriptClock;

impl TranscriptImportClock for FixedTranscriptClock {
    fn now_milliseconds(&self) -> i64 {
        1_800_000_000_000
    }
}

pub(crate) fn command(value: u64) -> CommandId {
    CommandId::from_parts(0, value)
}

pub(crate) fn transcript_json(episode_id: &str, first_text: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "id": "33333333-3333-3333-3333-333333333333",
        "episodeID": episode_id,
        "language": "en-US",
        "source": "publisher",
        "segments": [
            {
                "id": "44444444-4444-4444-4444-444444444444",
                "start": 47.125,
                "end": 53.0,
                "speakerID": "55555555-5555-5555-5555-555555555555",
                "text": first_text,
                "words": [
                    {"start": 47.125, "end": 47.6, "text": "Small"},
                    {"start": 47.65, "end": 48.1, "text": "habits"}
                ]
            },
            {
                "id": "66666666-6666-6666-6666-666666666666",
                "start": 53.0,
                "end": 60.0,
                "speakerID": null,
                "text": "when the cue is obvious."
            }
        ],
        "speakers": [{
            "id": "55555555-5555-5555-5555-555555555555",
            "label": "SPEAKER_00",
            "displayName": "Ada"
        }],
        "generatedAt": "2027-01-15T08:00:00Z"
    }))
    .unwrap()
}

fn create_artifact_schema(
    database: &Path,
    kind: LegacyTranscriptSourceKind,
    selected_path: &Path,
    bytes: &[u8],
) {
    let connection = Connection::open(database).unwrap();
    let selected_column = if kind == LegacyTranscriptSourceKind::ArtifactSqliteV1 {
        ", selected INTEGER NOT NULL"
    } else {
        ""
    };
    connection
        .execute_batch(&format!(
            "CREATE TABLE artifacts(\
             id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,subject_id TEXT NOT NULL,\
             input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,\
             location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,\
             verified_at REAL NOT NULL{selected_column},\
             UNIQUE(kind,subject_id,input_version,output_version));"
        ))
        .unwrap();
    if kind == LegacyTranscriptSourceKind::ArtifactSqliteV1 {
        connection.execute_batch(
            "CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);\
             INSERT INTO workflow_schema_versions VALUES('artifacts',1);",
        ).unwrap();
    }
    insert_artifact(
        &connection,
        kind,
        selected_path,
        bytes,
        "available",
        1_800_000_000.0,
    );
    if kind == LegacyTranscriptSourceKind::ArtifactSqliteV0 {
        connection
            .execute(
                "INSERT INTO artifacts(kind,subject_id,input_version,output_version,content_hash,\
             location,origin,schema_version,integrity,verified_at) VALUES('transcript',?1,\
             'newer-stale','newer-stale','00',?2,'publisher',1,'stale',?3)",
                params![
                    EPISODE_ID,
                    selected_path.to_string_lossy(),
                    1_900_000_000.0_f64
                ],
            )
            .unwrap();
    }
}

fn insert_artifact(
    connection: &Connection,
    kind: LegacyTranscriptSourceKind,
    selected_path: &Path,
    bytes: &[u8],
    integrity: &str,
    verified_at: f64,
) {
    let selected = if kind == LegacyTranscriptSourceKind::ArtifactSqliteV1 {
        ",selected"
    } else {
        ""
    };
    let selected_value = if kind == LegacyTranscriptSourceKind::ArtifactSqliteV1 {
        ",1"
    } else {
        ""
    };
    connection
        .execute(
            &format!(
                "INSERT INTO artifacts(kind,subject_id,input_version,output_version,content_hash,\
         location,origin,schema_version,integrity,verified_at{selected}) VALUES('transcript',?1,\
         'input-v1','output-v1',?2,?3,'publisher',1,?4,?5{selected_value})"
            ),
            params![
                EPISODE_ID,
                hex_digest(digest_bytes(bytes)),
                selected_path.to_string_lossy(),
                integrity,
                verified_at
            ],
        )
        .unwrap();
}
