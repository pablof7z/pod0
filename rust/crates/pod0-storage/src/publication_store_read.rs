use pod0_domain::{
    ContentDigest, EpisodeId, GeneratedArtifactId, PodcastId, PublicationFact, PublicationId,
    PublicationMediaEvidence, PublicationRecord, PublicationRouteId, StateRevision,
    UnixTimestampMilliseconds,
};
use rusqlite::{Connection, OptionalExtension, params};

use crate::publication_store_codec::{
    decode_artifact_kind, decode_fact_kind, decode_stage, fixed, optional_u64,
};
use crate::{PublicationStore, StorageError};

type PublicationRow = (
    Vec<u8>,
    Vec<u8>,
    i64,
    Option<i64>,
    Vec<u8>,
    Vec<u8>,
    i64,
    i64,
    String,
    String,
    String,
    String,
    i64,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<String>,
    String,
    i64,
    i64,
);

impl PublicationStore {
    pub fn publication(
        &self,
        publication_id: PublicationId,
    ) -> Result<Option<PublicationRecord>, StorageError> {
        self.read(|connection| read_publication(connection, publication_id))
    }

    pub fn page(
        &self,
        publication_id: Option<PublicationId>,
        offset: u32,
        maximum: u16,
    ) -> Result<Vec<PublicationRecord>, StorageError> {
        self.read(|connection| {
            if let Some(id) = publication_id {
                return Ok(read_publication(connection, id)?.into_iter().collect());
            }
            let limit = i64::from(maximum.clamp(1, 200));
            let mut statement = connection
                .prepare(
                    "SELECT publication_id FROM pod0_publications \
                     ORDER BY updated_at_ms DESC,publication_id DESC LIMIT ?1 OFFSET ?2",
                )
                .map_err(|error| StorageError::sqlite("prepare publication page", error))?;
            let ids = statement
                .query_map(params![limit, i64::from(offset)], |row| {
                    row.get::<_, Vec<u8>>(0)
                })
                .map_err(|error| StorageError::sqlite("read publication page", error))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| StorageError::sqlite("decode publication page", error))?;
            ids.into_iter()
                .map(|bytes| {
                    let id = PublicationId::from_bytes(fixed(bytes)?);
                    read_publication(connection, id)?.ok_or(StorageError::PublicationNotFound)
                })
                .collect()
        })
    }

    pub fn recoverable_publications(&self) -> Result<Vec<PublicationRecord>, StorageError> {
        self.page(None, 0, 200)
    }
}

pub(crate) fn read_publication(
    connection: &Connection,
    publication_id: PublicationId,
) -> Result<Option<PublicationRecord>, StorageError> {
    let row: Option<PublicationRow> = connection
        .query_row(
            "SELECT publication_id,artifact_id,artifact_kind_code,artifact_kind_wire_code,\
             episode_id,podcast_id,semantic_revision,state_revision,expected_author_hex,\
             correlation_token,public_media_url,media_type,media_byte_count,media_content_digest,\
             receipt_id,event_id_hex,stage_code,prepared_at_ms,updated_at_ms \
             FROM pod0_publications WHERE publication_id=?1",
            [publication_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    row.get(15)?,
                    row.get(16)?,
                    row.get(17)?,
                    row.get(18)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read publication", error))?;
    row.map(|row| decode_publication(connection, row))
        .transpose()
}

fn decode_publication(
    connection: &Connection,
    row: PublicationRow,
) -> Result<PublicationRecord, StorageError> {
    let publication_id = PublicationId::from_bytes(fixed(row.0)?);
    let facts = read_facts(connection, publication_id)?;
    Ok(PublicationRecord {
        publication_id,
        artifact_id: GeneratedArtifactId::from_bytes(fixed(row.1)?),
        artifact_kind: decode_artifact_kind(row.2, row.3)?,
        episode_id: EpisodeId::from_bytes(fixed(row.4)?),
        podcast_id: PodcastId::from_bytes(fixed(row.5)?),
        semantic_revision: u32::try_from(row.6).map_err(|_| malformed())?,
        revision: StateRevision::new(u64::try_from(row.7).map_err(|_| malformed())?),
        expected_author_hex: row.8,
        correlation_token: row.9,
        media: PublicationMediaEvidence {
            public_url: row.10,
            media_type: row.11,
            byte_count: u64::try_from(row.12).map_err(|_| malformed())?,
            content_digest: ContentDigest::from_bytes(fixed(row.13)?),
        },
        receipt_id: optional_u64(row.14)?,
        event_id_hex: row.15,
        stage: decode_stage(&row.16)?,
        prepared_at: UnixTimestampMilliseconds::new(row.17),
        updated_at: UnixTimestampMilliseconds::new(row.18),
        facts,
    })
}

fn read_facts(
    connection: &Connection,
    publication_id: PublicationId,
) -> Result<Vec<PublicationFact>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT sequence_number,fact_kind_code,route_id,attempt,event_id_hex,observed_at_ms,\
             detail FROM pod0_publication_facts WHERE publication_id=?1 ORDER BY sequence_number",
        )
        .map_err(|error| StorageError::sqlite("prepare publication facts", error))?;
    let rows = statement
        .query_map([publication_id.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read publication facts", error))?;
    rows.map(|row| {
        let row = row.map_err(|error| StorageError::sqlite("decode publication fact", error))?;
        Ok(PublicationFact {
            sequence: u64::try_from(row.0).map_err(|_| malformed())?,
            kind: decode_fact_kind(&row.1)?,
            route_id: row
                .2
                .map(|bytes| fixed(bytes).map(PublicationRouteId::from_bytes))
                .transpose()?,
            attempt: optional_u64(row.3)?,
            event_id_hex: row.4,
            observed_at: row.5.map(UnixTimestampMilliseconds::new),
            detail: row.6,
        })
    })
    .collect()
}

fn malformed() -> StorageError {
    StorageError::CorruptSchema {
        detail: "publication state is malformed",
    }
}
