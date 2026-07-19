use std::path::Path;

use pod0_domain::{TranscriptArtifact, TranscriptArtifactInput};
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::json;

use crate::LegacyTranscriptSourceKind;
use crate::listening_import_test_support::EPISODE_ID;
use crate::migration_db::configure;
use crate::transcript_import_digest::{digest_bytes, hex_digest};
use crate::transcript_store_write_rows::{
    ensure_semantic_document, insert_or_validate_artifact, require_episode_parent,
};

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

pub(super) fn create_artifact_schema(
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
        connection
            .execute_batch(
                "CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);\
                 INSERT INTO workflow_schema_versions VALUES('artifacts',1);",
            )
            .unwrap();
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

pub(crate) fn create_empty_artifact_schema(database: &Path) {
    Connection::open(database)
        .unwrap()
        .execute_batch(
            "CREATE TABLE artifacts(\
             id INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,subject_id TEXT NOT NULL,\
             input_version TEXT NOT NULL,output_version TEXT NOT NULL,content_hash TEXT NOT NULL,\
             location TEXT,origin TEXT,schema_version INTEGER NOT NULL,integrity TEXT NOT NULL,\
             verified_at REAL NOT NULL,selected INTEGER NOT NULL,\
             UNIQUE(kind,subject_id,input_version,output_version));\
             CREATE TABLE workflow_schema_versions(component TEXT PRIMARY KEY,version INTEGER NOT NULL);\
             INSERT INTO workflow_schema_versions VALUES('artifacts',1);",
        )
        .unwrap();
}

pub(crate) fn seed_pre_authority_selection(
    target: &Path,
    input: TranscriptArtifactInput,
) -> TranscriptArtifact {
    let artifact = TranscriptArtifact::seal(input).unwrap();
    let mut connection = Connection::open(target).unwrap();
    configure(&connection).unwrap();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    require_episode_parent(&transaction, &artifact).unwrap();
    ensure_semantic_document(&transaction, &artifact).unwrap();
    insert_or_validate_artifact(&transaction, &artifact, None, 1_800_000_000_000).unwrap();
    transaction
        .execute(
            "INSERT INTO pod0_transcript_selection(episode_id,artifact_id,transcript_version_id,\
             selection_revision,selected_at_ms,source_import_id) VALUES(?1,?2,?3,1,?4,NULL) \
             ON CONFLICT(episode_id) DO UPDATE SET artifact_id=excluded.artifact_id,\
             transcript_version_id=excluded.transcript_version_id,selection_revision=1,\
             selected_at_ms=excluded.selected_at_ms,source_import_id=NULL",
            params![
                artifact.episode_id.into_bytes().as_slice(),
                artifact.artifact_id.into_bytes().as_slice(),
                artifact.transcript_version_id.into_bytes().as_slice(),
                1_800_000_000_000_i64,
            ],
        )
        .unwrap();
    transaction
        .execute(
            "UPDATE pod0_transcript_state SET collection_revision=1,source_import_id=NULL \
             WHERE singleton=1",
            [],
        )
        .unwrap();
    transaction.commit().unwrap();
    artifact
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
