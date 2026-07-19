use std::fs;
use std::path::{Path, PathBuf};

use pod0_domain::{TranscriptArtifact, TranscriptArtifactId};
use rusqlite::{Connection, params};

use crate::transcript_import_digest::{digest_bytes, hex_digest};
use crate::transcript_rollback_format::{
    ROLLBACK_FORMAT_VERSION, RollbackManifest, legacy_source, legacy_transcript_bytes,
    manifest_entry, uuid_string,
};
use crate::transcript_store_codec::{artifact_id, stored_u64};
use crate::transcript_store_read_artifact::read_artifact_by_id;
use crate::{CURRENT_SCHEMA_VERSION, StorageError, TranscriptStore};

const SELECTION_DATABASE: &str = "transcript-selection.sqlite";
const MANIFEST_FILE: &str = "manifest.json";

#[path = "transcript_rollback_verify.rs"]
mod verification;
use verification::{verify_bundle, verify_bundle_files};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptRollbackExportReport {
    pub bundle_path: PathBuf,
    pub core_schema_version: u32,
    pub transcript_revision: u64,
    pub artifact_count: u32,
    pub selected_count: u32,
    pub reused_existing: bool,
}

struct ExportedTranscript {
    artifact: TranscriptArtifact,
    is_selected: bool,
}

pub fn export_transcript_rollback_bundle(
    target_path: &Path,
    export_root: &Path,
) -> Result<TranscriptRollbackExportReport, StorageError> {
    let store = TranscriptStore::open_authoritative(target_path)?;
    let (revision, artifacts) = authoritative_artifacts(&store)?;
    let artifact_count = checked_count(artifacts.len(), "transcript rollback export count")?;
    let selected_count = checked_count(
        artifacts.iter().filter(|entry| entry.is_selected).count(),
        "selected transcript rollback export count",
    )?;
    fs::create_dir_all(export_root)
        .map_err(|error| StorageError::io("create transcript rollback export root", error))?;
    let export_root = fs::canonicalize(export_root)
        .map_err(|error| StorageError::io("resolve transcript rollback export root", error))?;
    let bundle_path = export_root.join(format!(
        "transcripts-v{ROLLBACK_FORMAT_VERSION}-core-v{CURRENT_SCHEMA_VERSION}-revision-{revision}"
    ));
    if bundle_path.exists() {
        verify_bundle(&bundle_path, revision, &artifacts)?;
        return Ok(report(
            bundle_path,
            revision,
            artifact_count,
            selected_count,
            true,
        ));
    }

    let staging = tempfile::Builder::new()
        .prefix(".pod0-transcript-rollback-")
        .tempdir_in(&export_root)
        .map_err(|error| StorageError::io("stage transcript rollback export", error))?;
    write_bundle(staging.path(), &bundle_path, revision, &artifacts)?;
    match fs::rename(staging.path(), &bundle_path) {
        Ok(()) => {
            verify_bundle(&bundle_path, revision, &artifacts)?;
            Ok(report(
                bundle_path,
                revision,
                artifact_count,
                selected_count,
                false,
            ))
        }
        Err(_) if bundle_path.exists() => {
            verify_bundle(&bundle_path, revision, &artifacts)?;
            Ok(report(
                bundle_path,
                revision,
                artifact_count,
                selected_count,
                true,
            ))
        }
        Err(error) => Err(StorageError::io(
            "publish transcript rollback export",
            error,
        )),
    }
}

fn authoritative_artifacts(
    store: &TranscriptStore,
) -> Result<(u64, Vec<ExportedTranscript>), StorageError> {
    store.read(|connection| {
        let raw_revision: i64 = connection
            .query_row(
                "SELECT collection_revision FROM pod0_transcript_state WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .map_err(|error| StorageError::sqlite("read transcript export revision", error))?;
        let revision = stored_u64(raw_revision, "transcript export revision")?;
        let mut statement = connection
            .prepare(
                "SELECT a.artifact_id,CASE WHEN s.artifact_id=a.artifact_id THEN 1 ELSE 0 END \
                 FROM pod0_transcript_artifacts a LEFT JOIN pod0_transcript_selection s \
                 ON s.episode_id=a.episode_id ORDER BY a.episode_id,a.artifact_id",
            )
            .map_err(|error| StorageError::sqlite("prepare transcript rollback export", error))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, bool>(1)?))
            })
            .map_err(|error| StorageError::sqlite("read transcript rollback artifacts", error))?;
        let mut artifacts = Vec::new();
        for row in rows {
            let (bytes, is_selected) =
                row.map_err(|error| StorageError::sqlite("decode transcript rollback row", error))?;
            let id: TranscriptArtifactId = artifact_id(&bytes)?;
            artifacts.push(ExportedTranscript {
                artifact: read_artifact_by_id(connection, id)?
                    .ok_or(StorageError::TranscriptNotFound)?,
                is_selected,
            });
        }
        Ok((revision, artifacts))
    })
}

fn write_bundle(
    staging: &Path,
    final_path: &Path,
    revision: u64,
    artifacts: &[ExportedTranscript],
) -> Result<(), StorageError> {
    fs::create_dir(staging.join("transcripts"))
        .map_err(|error| StorageError::io("create rollback transcript directory", error))?;
    let mut entries = Vec::with_capacity(artifacts.len());
    let mut exported = Vec::with_capacity(artifacts.len());
    for entry in artifacts {
        let bytes = legacy_transcript_bytes(&entry.artifact)?;
        let relative_path = artifact_relative_path(&entry.artifact);
        let path = staging.join(&relative_path);
        fs::create_dir_all(
            path.parent()
                .ok_or(StorageError::InvalidTranscriptArtifact)?,
        )
        .map_err(|error| StorageError::io("create rollback artifact directory", error))?;
        fs::write(&path, &bytes)
            .map_err(|error| StorageError::io("write rollback transcript", error))?;
        entries.push(manifest_entry(
            &entry.artifact,
            &bytes,
            entry.is_selected,
            path_string(&relative_path)?,
        ));
        exported.push(bytes);
    }
    write_selection_database(
        &staging.join(SELECTION_DATABASE),
        final_path,
        revision,
        artifacts,
        &exported,
    )?;
    let manifest = RollbackManifest {
        format_version: ROLLBACK_FORMAT_VERSION,
        core_schema_version: CURRENT_SCHEMA_VERSION,
        transcript_revision: revision,
        entries,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| StorageError::InvalidTranscriptArtifact)?;
    fs::write(staging.join(MANIFEST_FILE), bytes)
        .map_err(|error| StorageError::io("write transcript rollback manifest", error))?;
    verify_bundle_files(staging, final_path, revision, artifacts)
}

fn write_selection_database(
    path: &Path,
    final_path: &Path,
    revision: u64,
    artifacts: &[ExportedTranscript],
    exported: &[Vec<u8>],
) -> Result<(), StorageError> {
    let mut connection = Connection::open(path)
        .map_err(|error| StorageError::sqlite("create rollback selection database", error))?;
    connection
        .execute_batch(
            "CREATE TABLE episodes(id TEXT PRIMARY KEY,subscription_id TEXT NOT NULL);\
             CREATE TABLE artifacts(id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,\
             subject_id TEXT NOT NULL,input_version TEXT NOT NULL,output_version TEXT NOT NULL,\
             content_hash TEXT NOT NULL,location TEXT,origin TEXT,schema_version INTEGER NOT NULL,\
             integrity TEXT NOT NULL,verified_at REAL NOT NULL,selected INTEGER NOT NULL,\
             UNIQUE(kind,subject_id,input_version,output_version));\
             CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);\
             INSERT INTO workflow_schema_versions VALUES('artifacts',1);\
             CREATE TABLE persistence_metadata(key TEXT PRIMARY KEY,value BLOB NOT NULL);",
        )
        .map_err(|error| StorageError::sqlite("initialize rollback selection database", error))?;
    let transaction = connection
        .transaction()
        .map_err(|error| StorageError::sqlite("begin rollback selection database", error))?;
    transaction
        .execute(
            "INSERT INTO persistence_metadata(key,value) VALUES('generation',?1)",
            [revision.to_string()],
        )
        .map_err(|error| StorageError::sqlite("write rollback export revision", error))?;
    for (entry, bytes) in artifacts.iter().zip(exported) {
        let artifact = &entry.artifact;
        let episode = uuid_string(artifact.episode_id.into_bytes());
        let podcast = uuid_string(artifact.podcast_id.into_bytes());
        transaction
            .execute(
                "INSERT OR IGNORE INTO episodes(id,subscription_id) VALUES(?1,?2)",
                params![episode, podcast],
            )
            .map_err(|error| StorageError::sqlite("write rollback episode parent", error))?;
        let location = final_path.join(artifact_relative_path(artifact));
        transaction
            .execute(
                "INSERT INTO artifacts(kind,subject_id,input_version,output_version,content_hash,\
                 location,origin,schema_version,integrity,verified_at,selected)\
                 VALUES('transcript',?1,?2,?3,?4,?5,?6,1,'available',?7,?8)",
                params![
                    episode,
                    artifact.source_revision,
                    hex_id(artifact.artifact_id.into_bytes()),
                    hex_digest(digest_bytes(bytes)),
                    path_string(&location)?,
                    legacy_source(artifact.provenance.source)?,
                    artifact.generated_at.value() as f64 / 1_000.0,
                    entry.is_selected,
                ],
            )
            .map_err(|error| StorageError::sqlite("write rollback transcript artifact", error))?;
    }
    transaction
        .commit()
        .map_err(|error| StorageError::sqlite("commit rollback selection database", error))
}

fn artifact_relative_path(artifact: &TranscriptArtifact) -> PathBuf {
    PathBuf::from("transcripts")
        .join("artifacts")
        .join(uuid_string(artifact.episode_id.into_bytes()))
        .join(format!(
            "{}.json",
            hex_id(artifact.artifact_id.into_bytes())
        ))
}

fn checked_count(value: usize, entity: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| StorageError::ImportLimitExceeded { entity })
}

fn path_string(path: &Path) -> Result<String, StorageError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or(StorageError::InvalidTranscriptArtifact)
}

fn report(
    bundle_path: PathBuf,
    transcript_revision: u64,
    artifact_count: u32,
    selected_count: u32,
    reused_existing: bool,
) -> TranscriptRollbackExportReport {
    TranscriptRollbackExportReport {
        bundle_path,
        core_schema_version: CURRENT_SCHEMA_VERSION,
        transcript_revision,
        artifact_count,
        selected_count,
        reused_existing,
    }
}

fn hex_id(bytes: [u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
