use pod0_domain::{
    ArtifactReference, DownloadArtifactStatus, EpisodeId, EpisodeListeningState, EpisodeRecord,
    PodcastId, TranscriptArtifactStatus, UnixTimestampMilliseconds,
};
use rusqlite::{Connection, Row};

use crate::StorageError;
use crate::library_feed_codec;
use crate::listening_db_codec::{corrupt, decode_completion, decode_transcript_source};

pub(crate) fn read_episodes(connection: &Connection) -> Result<Vec<EpisodeRecord>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT e.episode_id,e.podcast_id,e.publisher_guid,e.title,e.description,e.published_at_ms,\
         e.duration_ms,e.enclosure_url,e.enclosure_mime_type,e.image_url,e.resume_position_ms,\
         e.completion_code,e.completion_cause_code,e.completion_cause_wire_code,e.is_starred,\
         e.download_code,e.download_wire_code,e.download_ref_version,e.download_ref_key,\
         e.download_byte_count,e.transcript_code,e.transcript_wire_code,e.transcript_ref_version,\
         e.transcript_ref_key,e.transcript_source_code,e.transcript_source_wire_code,\
         m.publisher_transcript_url,m.publisher_transcript_media_type,\
         m.publisher_transcript_format_code,m.publisher_transcript_format_wire_code,\
         m.chapters_url,m.persons_json,m.sound_bites_json \
         FROM pod0_episodes e JOIN pod0_podcasts p ON p.podcast_id=e.podcast_id \
         LEFT JOIN pod0_episode_feed_metadata m ON m.episode_id=e.episode_id \
         WHERE p.library_visible=1 \
         ORDER BY e.rowid",
    ).map_err(|error| StorageError::sqlite("prepare episode projection", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| StorageError::sqlite("read episode projection", error))?;
    let mut episodes = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| StorageError::sqlite("read episode projection", error))?
    {
        episodes.push(episode_from_row(row)?);
    }
    Ok(episodes)
}

fn episode_from_row(row: &Row<'_>) -> Result<EpisodeRecord, StorageError> {
    Ok(EpisodeRecord {
        episode_id: EpisodeId::from_bytes(id(row, 0)?),
        podcast_id: PodcastId::from_bytes(id(row, 1)?),
        publisher_guid: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        published_at: UnixTimestampMilliseconds::new(row.get(5)?),
        duration_milliseconds: optional_unsigned(row.get(6)?, "duration")?,
        enclosure_url: row.get(7)?,
        enclosure_mime_type: row.get(8)?,
        image_url: row.get(9)?,
        feed_metadata: library_feed_codec::decode(
            row.get(26)?,
            row.get(27)?,
            row.get(28)?,
            row.get(29)?,
            row.get(30)?,
            row.get(31)?,
            row.get(32)?,
        )?,
        listening: EpisodeListeningState {
            resume_position_milliseconds: unsigned(row.get(10)?, "resume position")?,
            completion: decode_completion(row.get(11)?, row.get(12)?, row.get(13)?)?,
        },
        is_starred: boolean(row.get(14)?)?,
        download: decode_download(row)?,
        transcript: decode_transcript(row)?,
    })
}

fn decode_download(row: &Row<'_>) -> Result<DownloadArtifactStatus, StorageError> {
    match row.get::<_, i64>(15)? {
        1 => Ok(DownloadArtifactStatus::Unavailable),
        2 => Ok(DownloadArtifactStatus::Available {
            reference: artifact(row.get(17)?, row.get(18)?)?,
            byte_count: unsigned(row.get(19)?, "download byte count")?,
        }),
        255 => Ok(DownloadArtifactStatus::Unsupported {
            wire_code: count(
                row.get::<_, Option<i64>>(16)?
                    .ok_or_else(|| corrupt("download wire code"))?,
                "download wire code",
            )?,
        }),
        _ => Err(corrupt("download code")),
    }
}

fn decode_transcript(row: &Row<'_>) -> Result<TranscriptArtifactStatus, StorageError> {
    match row.get::<_, i64>(20)? {
        1 => Ok(TranscriptArtifactStatus::Unavailable),
        2 => Ok(TranscriptArtifactStatus::Available {
            reference: artifact(row.get(22)?, row.get(23)?)?,
            source: decode_transcript_source(
                row.get::<_, Option<i64>>(24)?
                    .ok_or_else(|| corrupt("transcript source"))?,
                row.get(25)?,
            )?,
        }),
        255 => Ok(TranscriptArtifactStatus::Unsupported {
            wire_code: count(
                row.get::<_, Option<i64>>(21)?
                    .ok_or_else(|| corrupt("transcript wire code"))?,
                "transcript wire code",
            )?,
        }),
        _ => Err(corrupt("transcript code")),
    }
}

fn artifact(version: Option<i64>, key: Option<String>) -> Result<ArtifactReference, StorageError> {
    Ok(ArtifactReference {
        schema_version: count(
            version.ok_or_else(|| corrupt("artifact version"))?,
            "artifact version",
        )?,
        opaque_key: key.ok_or_else(|| corrupt("artifact key"))?,
    })
}
fn id(row: &Row<'_>, index: usize) -> Result<[u8; 16], StorageError> {
    row.get::<_, Vec<u8>>(index)?
        .try_into()
        .map_err(|_| corrupt("stored ID length"))
}
fn unsigned(value: i64, detail: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| corrupt(detail))
}
fn optional_unsigned(
    value: Option<i64>,
    detail: &'static str,
) -> Result<Option<u64>, StorageError> {
    value.map(|value| unsigned(value, detail)).transpose()
}
fn count(value: i64, detail: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| corrupt(detail))
}
fn boolean(value: i64) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt("boolean")),
    }
}
