use pod0_domain::{
    CommandId, DownloadArtifactStatus, ListeningDomainSnapshot, TranscriptArtifactStatus,
};
use rusqlite::{Transaction, params};

use crate::StorageError;
use crate::import_model::InspectedLegacySource;
use crate::library_feed_codec;
use crate::listening_db_codec::{
    auto_download, bool_value, completion, download, i64_value, podcast_kind, transcript,
    transcript_source,
};

pub(crate) fn insert_podcasts(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    snapshot: &ListeningDomainSnapshot,
) -> Result<(), StorageError> {
    let mut statement = transaction.prepare(
        "INSERT INTO pod0_podcasts(podcast_id,kind_code,kind_wire_code,feed_url,feed_key_v1,title,author,image_url,description,language,categories_json,discovered_at_ms,title_is_placeholder,last_refreshed_at_ms,etag,last_modified,source_import_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
    ).map_err(|error| StorageError::sqlite("prepare podcast import", error))?;
    for podcast in &snapshot.podcasts {
        let (kind, wire) = podcast_kind(&podcast.kind);
        let categories = serde_json::to_string(&podcast.categories).map_err(|_| {
            StorageError::CorruptSchema {
                detail: "podcast categories cannot be encoded",
            }
        })?;
        statement
            .execute(params![
                podcast.podcast_id.into_bytes().as_slice(),
                kind,
                wire,
                podcast
                    .feed_identity
                    .as_ref()
                    .map(|feed| feed.source_url.as_str()),
                podcast
                    .feed_identity
                    .as_ref()
                    .map(|feed| feed.comparison_key.as_str()),
                podcast.title,
                podcast.author,
                podcast.image_url,
                podcast.description,
                podcast.language,
                categories,
                podcast.discovered_at.value,
                bool_value(podcast.title_is_placeholder),
                podcast.last_refreshed_at.map(|value| value.value),
                podcast.etag,
                podcast.last_modified,
                import_id.into_bytes().as_slice(),
            ])
            .map_err(|error| StorageError::sqlite("insert podcast", error))?;
    }
    Ok(())
}

pub(crate) fn insert_subscriptions(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    snapshot: &ListeningDomainSnapshot,
) -> Result<(), StorageError> {
    let mut statement = transaction.prepare(
        "INSERT INTO pod0_subscriptions(podcast_id,subscribed_at_ms,auto_download_code,auto_download_wire_code,auto_download_latest_count,wifi_only,notifications_enabled,default_playback_rate_permille,source_import_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
    ).map_err(|error| StorageError::sqlite("prepare subscription import", error))?;
    for subscription in &snapshot.subscriptions {
        let (mode, wire, latest) = auto_download(&subscription.auto_download.mode);
        statement
            .execute(params![
                subscription.podcast_id.into_bytes().as_slice(),
                subscription.subscribed_at.value,
                mode,
                wire,
                latest,
                bool_value(subscription.auto_download.wifi_only),
                bool_value(subscription.notifications_enabled),
                subscription
                    .default_playback_rate
                    .map(|rate| i64::from(rate.value)),
                import_id.into_bytes().as_slice(),
            ])
            .map_err(|error| StorageError::sqlite("insert subscription", error))?;
    }
    Ok(())
}

pub(crate) fn insert_episodes(
    transaction: &Transaction<'_>,
    import_id: CommandId,
    source: &InspectedLegacySource,
) -> Result<(), StorageError> {
    let mut statement = transaction.prepare(
        "INSERT INTO pod0_episodes(episode_id,podcast_id,publisher_guid,title,description,published_at_ms,duration_ms,enclosure_url,enclosure_mime_type,image_url,resume_position_ms,completion_code,completion_cause_code,completion_cause_wire_code,is_starred,download_code,download_wire_code,download_ref_version,download_ref_key,download_byte_count,transcript_code,transcript_wire_code,transcript_ref_version,transcript_ref_key,transcript_source_code,transcript_source_wire_code,legacy_payload,source_import_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28)",
    ).map_err(|error| StorageError::sqlite("prepare episode import", error))?;
    let mut metadata_statement = transaction
        .prepare(
            "INSERT INTO pod0_episode_feed_metadata(episode_id,publisher_transcript_url,\
         publisher_transcript_media_type,publisher_transcript_format_code,\
         publisher_transcript_format_wire_code,chapters_url,persons_json,sound_bites_json) \
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        )
        .map_err(|error| StorageError::sqlite("prepare episode feed metadata import", error))?;
    for (episode, payload) in source
        .snapshot
        .episodes
        .iter()
        .zip(&source.episode_payloads)
    {
        let (completion_code, completion_cause, completion_wire) =
            completion(&episode.listening.completion);
        let (download_code, download_wire) = download(&episode.download);
        let (download_version, download_key, download_bytes) = match &episode.download {
            DownloadArtifactStatus::Available {
                reference,
                byte_count,
            } => (
                Some(i64::from(reference.schema_version)),
                Some(reference.opaque_key.as_str()),
                Some(i64_value(*byte_count, "download byte count")?),
            ),
            _ => (None, None, None),
        };
        let (transcript_code, transcript_wire) = transcript(&episode.transcript);
        let (transcript_version, transcript_key, source_code, source_wire) =
            match &episode.transcript {
                TranscriptArtifactStatus::Available { reference, source } => {
                    let (code, wire) = transcript_source(source);
                    (
                        Some(i64::from(reference.schema_version)),
                        Some(reference.opaque_key.as_str()),
                        Some(code),
                        wire,
                    )
                }
                _ => (None, None, None, None),
            };
        statement
            .execute(params![
                episode.episode_id.into_bytes().as_slice(),
                episode.podcast_id.into_bytes().as_slice(),
                episode.publisher_guid,
                episode.title,
                episode.description,
                episode.published_at.value,
                episode
                    .duration_milliseconds
                    .map(|value| i64_value(value, "duration"))
                    .transpose()?,
                episode.enclosure_url,
                episode.enclosure_mime_type,
                episode.image_url,
                i64_value(
                    episode.listening.resume_position_milliseconds,
                    "resume position"
                )?,
                completion_code,
                completion_cause,
                completion_wire,
                bool_value(episode.is_starred),
                download_code,
                download_wire,
                download_version,
                download_key,
                download_bytes,
                transcript_code,
                transcript_wire,
                transcript_version,
                transcript_key,
                source_code,
                source_wire,
                payload,
                import_id.into_bytes().as_slice(),
            ])
            .map_err(|error| StorageError::sqlite("insert episode", error))?;
        let metadata = library_feed_codec::encode(&episode.feed_metadata)?;
        metadata_statement
            .execute(params![
                episode.episode_id.into_bytes().as_slice(),
                metadata.transcript_url,
                metadata.transcript_media_type,
                metadata.transcript_format_code,
                metadata.transcript_format_wire_code,
                metadata.chapters_url,
                metadata.persons_json,
                metadata.sound_bites_json,
            ])
            .map_err(|error| StorageError::sqlite("insert episode feed metadata", error))?;
    }
    Ok(())
}
