use std::fs;
use std::path::PathBuf;

use pod0_domain::CommandId;
use rusqlite::{Connection, params};

use crate::listening_import_test_support::{
    EPISODE_ID, ImportFixture, create_sqlite_source, current_metadata, episode,
};
use crate::transcript_import_digest::{digest_bytes, hex_digest};
use crate::{
    LegacyTranscriptSourceKind, StorageError, TranscriptImportClock, TranscriptImportPlan,
    TranscriptImportReport, TranscriptImportVerification, TranscriptImporter,
    commit_listening_cutover, inspect_legacy_transcript_source,
};

#[path = "transcript_import_fixture_sqlite.rs"]
mod sqlite;
use sqlite::create_artifact_schema;
pub(crate) use sqlite::{
    create_empty_artifact_schema, seed_pre_authority_selection, transcript_json,
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
        commit_listening_cutover(&import.target, 1_800_000_000_000).unwrap();
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

    pub(crate) fn add_current_artifact(
        &self,
        episode_id: &str,
        file_name: &str,
        first_text: &str,
        input_version: &str,
        output_version: &str,
        selected: bool,
    ) -> PathBuf {
        let path = self.transcript_root.join(file_name);
        let bytes = transcript_json(episode_id, first_text);
        fs::write(&path, &bytes).unwrap();
        Connection::open(&self.import.source)
            .unwrap()
            .execute(
                "INSERT INTO artifacts(kind,subject_id,input_version,output_version,content_hash,\
                 location,origin,schema_version,integrity,verified_at,selected) \
                 VALUES('transcript',?1,?2,?3,?4,?5,'publisher',1,'available',?6,?7)",
                params![
                    episode_id,
                    input_version,
                    output_version,
                    hex_digest(digest_bytes(&bytes)),
                    path.to_string_lossy(),
                    1_700_000_000.0_f64,
                    selected,
                ],
            )
            .unwrap();
        path
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
