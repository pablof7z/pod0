use pod0_domain::{AdSpanId, ChapterId, CommandId};
use rusqlite::Connection;

use crate::chapter_store_codec::{ad_span_id, chapter_id};
use crate::{InspectedChapterSource, StorageError};

pub(crate) fn verify_import_identity_evidence(
    connection: &Connection,
    import_id: CommandId,
    source: &InspectedChapterSource,
) -> Result<(), StorageError> {
    for entry in &source.entries {
        let chapters = read_chapter_identities(connection, import_id, entry.evidence_id)?;
        let expected_chapters = entry
            .legacy_chapters
            .iter()
            .map(|value| {
                (
                    value.ordinal,
                    value.legacy_id,
                    value.is_ai_generated,
                    value.chapter_id,
                )
            })
            .collect::<Vec<_>>();
        if chapters != expected_chapters {
            return Err(StorageError::InvalidChapterArtifact);
        }
        let ad_spans = read_ad_identities(connection, import_id, entry.evidence_id)?;
        let expected_ad_spans = entry
            .legacy_ad_spans
            .iter()
            .map(|value| (value.ordinal, value.legacy_id, value.ad_span_id))
            .collect::<Vec<_>>();
        if ad_spans != expected_ad_spans {
            return Err(StorageError::InvalidChapterArtifact);
        }
    }
    Ok(())
}

type StoredChapterIdentity = (u32, Option<[u8; 16]>, bool, Option<ChapterId>);

fn read_chapter_identities(
    connection: &Connection,
    import_id: CommandId,
    evidence_id: pod0_domain::ContentDigest,
) -> Result<Vec<StoredChapterIdentity>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT ordinal,legacy_id,legacy_is_ai_generated,chapter_id \
             FROM pod0_chapter_import_chapter_evidence WHERE import_id=?1 AND entry_id=?2 \
             ORDER BY ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare chapter identity evidence", error))?;
    let rows = statement
        .query_map(
            [
                import_id.into_bytes().as_slice(),
                evidence_id.into_bytes().as_slice(),
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                ))
            },
        )
        .map_err(|error| StorageError::sqlite("read chapter identity evidence", error))?;
    rows.map(|row| {
        let row =
            row.map_err(|error| StorageError::sqlite("decode chapter identity evidence", error))?;
        Ok((
            stored_ordinal(row.0)?,
            row.1.as_deref().map(stored_uuid).transpose()?,
            row.2,
            row.3.as_deref().map(chapter_id).transpose()?,
        ))
    })
    .collect()
}

type StoredAdIdentity = (u32, Option<[u8; 16]>, Option<AdSpanId>);

fn read_ad_identities(
    connection: &Connection,
    import_id: CommandId,
    evidence_id: pod0_domain::ContentDigest,
) -> Result<Vec<StoredAdIdentity>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT ordinal,legacy_id,ad_span_id FROM pod0_chapter_import_ad_evidence \
             WHERE import_id=?1 AND entry_id=?2 ORDER BY ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare ad-span identity evidence", error))?;
    let rows = statement
        .query_map(
            [
                import_id.into_bytes().as_slice(),
                evidence_id.into_bytes().as_slice(),
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            },
        )
        .map_err(|error| StorageError::sqlite("read ad-span identity evidence", error))?;
    rows.map(|row| {
        let row =
            row.map_err(|error| StorageError::sqlite("decode ad-span identity evidence", error))?;
        Ok((
            stored_ordinal(row.0)?,
            row.1.as_deref().map(stored_uuid).transpose()?,
            row.2.as_deref().map(ad_span_id).transpose()?,
        ))
    })
    .collect()
}

fn stored_ordinal(value: i64) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| StorageError::InvalidChapterArtifact)
}

fn stored_uuid(value: &[u8]) -> Result<[u8; 16], StorageError> {
    value
        .try_into()
        .map_err(|_| StorageError::InvalidChapterArtifact)
}
