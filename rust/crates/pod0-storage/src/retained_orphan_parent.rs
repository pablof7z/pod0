use pod0_domain::{EpisodeId, PodcastId};
use rusqlite::{Transaction, params};

use crate::StorageError;

pub(crate) const fn retained_orphan_podcast_id() -> PodcastId {
    PodcastId::from_bytes([
        0x86, 0x36, 0x3c, 0xd2, 0x4b, 0x10, 0xa8, 0x8f, 0x98, 0x73, 0x54, 0x58, 0x4e, 0xbb, 0xed,
        0x99,
    ])
}

/// Creates a hidden listening parent for durable artifacts whose original
/// episode disappeared before cutover. Transcript and chapter migration share
/// this identity so history for the same orphan episode cannot fork.
pub(crate) fn ensure_retained_orphan_parent(
    transaction: &Transaction<'_>,
    episode_id: EpisodeId,
    observed_at_ms: i64,
) -> Result<(), StorageError> {
    let source_import_id: Vec<u8> = transaction
        .query_row(
            "SELECT import_id FROM pod0_listening_imports ORDER BY verified_at_ms DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read retained orphan listening import", error))?;
    let podcast_id = retained_orphan_podcast_id();
    transaction
        .execute(
            "INSERT OR IGNORE INTO pod0_podcasts(podcast_id,kind_code,kind_wire_code,feed_url,\
             feed_key_v1,title,author,image_url,description,language,categories_json,\
             discovered_at_ms,title_is_placeholder,last_refreshed_at_ms,etag,last_modified,\
             source_import_id,library_visible) VALUES(?1,2,NULL,NULL,NULL,'Recovered artifacts',\
             '',NULL,'Artifacts retained after their library episode disappeared.',NULL,'[]',\
             ?2,1,NULL,NULL,NULL,?3,0)",
            params![
                podcast_id.into_bytes().as_slice(),
                observed_at_ms,
                source_import_id.as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("create retained orphan podcast", error))?;
    let episode_key = hex_id(episode_id.into_bytes());
    transaction
        .execute(
            "INSERT OR IGNORE INTO pod0_episodes(episode_id,podcast_id,publisher_guid,title,\
             description,published_at_ms,duration_ms,enclosure_url,enclosure_mime_type,image_url,\
             resume_position_ms,completion_code,completion_cause_code,completion_cause_wire_code,\
             is_starred,download_code,download_wire_code,download_ref_version,download_ref_key,\
             download_byte_count,transcript_code,transcript_wire_code,transcript_ref_version,\
             transcript_ref_key,transcript_source_code,transcript_source_wire_code,legacy_payload,\
             source_import_id) VALUES(?1,?2,?3,'Recovered artifact','',0,NULL,?4,NULL,NULL,0,1,\
             NULL,NULL,0,1,NULL,NULL,NULL,NULL,1,NULL,NULL,NULL,NULL,NULL,X'7B7D',?5)",
            params![
                episode_id.into_bytes().as_slice(),
                podcast_id.into_bytes().as_slice(),
                format!("retained-orphan:{episode_key}"),
                format!("pod0-retained-orphan://{episode_key}"),
                source_import_id.as_slice(),
            ],
        )
        .map_err(|error| StorageError::sqlite("create retained orphan episode", error))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO pod0_episode_feed_metadata(episode_id,persons_json,\
             sound_bites_json) VALUES(?1,'[]','[]')",
            [episode_id.into_bytes().as_slice()],
        )
        .map_err(|error| StorageError::sqlite("create retained orphan metadata", error))?;
    Ok(())
}

fn hex_id(bytes: [u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
