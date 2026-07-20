use pod0_domain::{
    AdSpanInput, ChapterArtifact, ChapterArtifactId, ChapterArtifactInput,
    ChapterArtifactProvenance, ChapterInput, ChapterLegacyProvenance, UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension};

use crate::StorageError;
use crate::chapter_store_codec::{
    ad_kind, artifact_id, digest, episode_id, evaluation, legacy_source, podcast_id, source,
    stored_u32, stored_u64, transcript_version_id,
};

#[allow(clippy::type_complexity)]
pub(crate) fn read_chapter_artifact(
    connection: &Connection,
    expected_id: ChapterArtifactId,
) -> Result<Option<ChapterArtifact>, StorageError> {
    let row = connection
        .query_row(
            "SELECT artifact_id,schema_version,content_digest,integrity_digest,episode_id,\
             podcast_id,source_revision,source_code,provider,model,policy_version,\
             source_payload_digest,transcript_version_id,transcript_content_digest,\
             generated_at_ms,duration_ms,chapter_count,ad_span_evaluation_code,ad_span_count,\
             legacy_source_code,legacy_original_origin,legacy_generated_at_was_unknown \
             FROM pod0_chapter_artifacts WHERE artifact_id=?1",
            [expected_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, Vec<u8>>(11)?,
                    row.get::<_, Option<Vec<u8>>>(12)?,
                    row.get::<_, Option<Vec<u8>>>(13)?,
                    row.get::<_, i64>(14)?,
                    row.get::<_, Option<i64>>(15)?,
                    row.get::<_, i64>(16)?,
                    row.get::<_, i64>(17)?,
                    row.get::<_, i64>(18)?,
                    row.get::<_, Option<i64>>(19)?,
                    row.get::<_, Option<String>>(20)?,
                    row.get::<_, Option<bool>>(21)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read chapter artifact", error))?;
    let Some(row) = row else { return Ok(None) };
    let stored_id = artifact_id(&row.0)?;
    let legacy_import = match (row.19, row.20, row.21) {
        (Some(source_code), original_origin, Some(generated_at_was_unknown)) => {
            Some(ChapterLegacyProvenance {
                source: legacy_source(source_code)?,
                original_origin,
                generated_at_was_unknown,
            })
        }
        (None, None, None) => None,
        _ => return Err(StorageError::InvalidChapterArtifact),
    };
    let chapters = read_chapters(connection, stored_id)?;
    let ad_spans = read_ad_spans(connection, stored_id)?;
    let artifact = ChapterArtifact::seal(ChapterArtifactInput {
        episode_id: episode_id(&row.4)?,
        podcast_id: podcast_id(&row.5)?,
        source_revision: row.6,
        provenance: ChapterArtifactProvenance {
            source: source(row.7)?,
            provider: row.8,
            model: row.9,
            policy_version: stored_u32(row.10, "chapter policy version")?,
            source_payload_digest: digest(&row.11)?,
            transcript_version_id: row.12.as_deref().map(transcript_version_id).transpose()?,
            transcript_content_digest: row.13.as_deref().map(digest).transpose()?,
            legacy_import,
        },
        generated_at: UnixTimestampMilliseconds::new(row.14),
        duration_milliseconds: row
            .15
            .map(|value| stored_u64(value, "chapter duration"))
            .transpose()?,
        chapters,
        ad_span_evaluation: evaluation(row.17)?,
        ad_spans,
    })
    .map_err(|_| StorageError::InvalidChapterArtifact)?;
    if stored_id != expected_id
        || artifact.artifact_id != stored_id
        || artifact.schema_version != stored_u32(row.1, "chapter schema version")?
        || artifact.content_digest != digest(&row.2)?
        || artifact.integrity_digest != digest(&row.3)?
        || artifact.chapters.len() as i64 != row.16
        || artifact.ad_spans.len() as i64 != row.18
    {
        return Err(StorageError::InvalidChapterArtifact);
    }
    Ok(Some(artifact))
}

fn read_chapters(
    connection: &Connection,
    artifact: ChapterArtifactId,
) -> Result<Vec<ChapterInput>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT start_ms,end_ms,title,summary,image_url,link_url,\
             include_in_table_of_contents,source_episode_id FROM pod0_chapter_items \
             WHERE artifact_id=?1 ORDER BY ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare chapter items", error))?;
    let rows = statement
        .query_map([artifact.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, Option<Vec<u8>>>(7)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read chapter items", error))?;
    rows.map(|row| {
        let row = row.map_err(|error| StorageError::sqlite("decode chapter item", error))?;
        Ok(ChapterInput {
            start_milliseconds: stored_u64(row.0, "chapter start")?,
            end_milliseconds: row
                .1
                .map(|value| stored_u64(value, "chapter end"))
                .transpose()?,
            title: row.2,
            summary: row.3,
            image_url: row.4,
            link_url: row.5,
            include_in_table_of_contents: row.6,
            source_episode_id: row.7.as_deref().map(episode_id).transpose()?,
        })
    })
    .collect()
}

fn read_ad_spans(
    connection: &Connection,
    artifact: ChapterArtifactId,
) -> Result<Vec<AdSpanInput>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT start_ms,end_ms,kind_code FROM pod0_ad_spans \
             WHERE artifact_id=?1 ORDER BY ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare chapter ad spans", error))?;
    let rows = statement
        .query_map([artifact.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read chapter ad spans", error))?;
    rows.map(|row| {
        let row = row.map_err(|error| StorageError::sqlite("decode chapter ad span", error))?;
        Ok(AdSpanInput {
            start_milliseconds: stored_u64(row.0, "chapter ad start")?,
            end_milliseconds: stored_u64(row.1, "chapter ad end")?,
            kind: ad_kind(row.2)?,
        })
    })
    .collect()
}
