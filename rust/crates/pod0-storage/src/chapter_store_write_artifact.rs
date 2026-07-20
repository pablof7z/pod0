use pod0_domain::{ChapterArtifact, CommandId};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::StorageError;
use crate::chapter_store_codec::{
    ad_kind_code, evaluation_code, legacy_source_code, source_code, sqlite_i64,
};
use crate::chapter_store_read_artifact::read_chapter_artifact;

pub(crate) fn insert_or_validate_chapter_artifact(
    transaction: &Transaction<'_>,
    artifact: &ChapterArtifact,
    source_import_id: Option<CommandId>,
    created_at_ms: i64,
) -> Result<(), StorageError> {
    artifact
        .verify_integrity()
        .map_err(|_| StorageError::InvalidChapterArtifact)?;
    let exists: Option<i64> = transaction
        .query_row(
            "SELECT 1 FROM pod0_chapter_artifacts WHERE artifact_id=?1",
            [artifact.artifact_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read existing chapter artifact", error))?;
    if exists.is_some() {
        return if read_chapter_artifact(transaction, artifact.artifact_id)?
            == Some(artifact.clone())
        {
            Ok(())
        } else {
            Err(StorageError::InvalidChapterArtifact)
        };
    }
    let legacy = artifact.provenance.legacy_import.as_ref();
    transaction
        .execute(
            "INSERT INTO pod0_chapter_artifacts(artifact_id,schema_version,content_digest,\
             integrity_digest,episode_id,podcast_id,source_revision,source_code,provider,model,\
             policy_version,source_payload_digest,transcript_version_id,transcript_content_digest,\
             generated_at_ms,duration_ms,chapter_count,ad_span_evaluation_code,ad_span_count,\
             legacy_source_code,legacy_original_origin,legacy_generated_at_was_unknown,\
             source_import_id,created_at_ms) VALUES(\
             ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,\
             ?20,?21,?22,?23,?24)",
            params![
                artifact.artifact_id.into_bytes().as_slice(),
                artifact.schema_version,
                artifact.content_digest.into_bytes().as_slice(),
                artifact.integrity_digest.into_bytes().as_slice(),
                artifact.episode_id.into_bytes().as_slice(),
                artifact.podcast_id.into_bytes().as_slice(),
                artifact.source_revision,
                source_code(artifact.provenance.source)?,
                artifact.provenance.provider,
                artifact.provenance.model,
                artifact.provenance.policy_version,
                artifact
                    .provenance
                    .source_payload_digest
                    .into_bytes()
                    .as_slice(),
                artifact
                    .provenance
                    .transcript_version_id
                    .map(|value| value.into_bytes().to_vec()),
                artifact
                    .provenance
                    .transcript_content_digest
                    .map(|value| value.into_bytes().to_vec()),
                artifact.generated_at.value,
                artifact.duration_milliseconds.map(sqlite_i64).transpose()?,
                i64::try_from(artifact.chapters.len())
                    .map_err(|_| StorageError::InvalidChapterArtifact)?,
                evaluation_code(artifact.ad_span_evaluation)?,
                i64::try_from(artifact.ad_spans.len())
                    .map_err(|_| StorageError::InvalidChapterArtifact)?,
                legacy
                    .map(|value| legacy_source_code(value.source))
                    .transpose()?,
                legacy.and_then(|value| value.original_origin.as_deref()),
                legacy.map(|value| value.generated_at_was_unknown),
                source_import_id.map(|value| value.into_bytes().to_vec()),
                created_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert chapter artifact", error))?;
    insert_chapters(transaction, artifact)?;
    insert_ad_spans(transaction, artifact)
}

fn insert_chapters(
    transaction: &Transaction<'_>,
    artifact: &ChapterArtifact,
) -> Result<(), StorageError> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO pod0_chapter_items(chapter_id,artifact_id,ordinal,start_ms,end_ms,title,\
             summary,image_url,link_url,include_in_table_of_contents,source_episode_id) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        )
        .map_err(|error| StorageError::sqlite("prepare chapter items", error))?;
    for chapter in &artifact.chapters {
        statement
            .execute(params![
                chapter.chapter_id.into_bytes().as_slice(),
                artifact.artifact_id.into_bytes().as_slice(),
                chapter.ordinal,
                sqlite_i64(chapter.start_milliseconds)?,
                chapter.end_milliseconds.map(sqlite_i64).transpose()?,
                chapter.title,
                chapter.summary,
                chapter.image_url,
                chapter.link_url,
                chapter.include_in_table_of_contents,
                chapter
                    .source_episode_id
                    .map(|value| value.into_bytes().to_vec()),
            ])
            .map_err(|error| StorageError::sqlite("insert chapter item", error))?;
    }
    Ok(())
}

fn insert_ad_spans(
    transaction: &Transaction<'_>,
    artifact: &ChapterArtifact,
) -> Result<(), StorageError> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO pod0_ad_spans(ad_span_id,artifact_id,ordinal,start_ms,end_ms,kind_code) \
             VALUES(?1,?2,?3,?4,?5,?6)",
        )
        .map_err(|error| StorageError::sqlite("prepare chapter ad spans", error))?;
    for span in &artifact.ad_spans {
        statement
            .execute(params![
                span.ad_span_id.into_bytes().as_slice(),
                artifact.artifact_id.into_bytes().as_slice(),
                span.ordinal,
                sqlite_i64(span.start_milliseconds)?,
                sqlite_i64(span.end_milliseconds)?,
                ad_kind_code(span.kind)?,
            ])
            .map_err(|error| StorageError::sqlite("insert chapter ad span", error))?;
    }
    Ok(())
}
