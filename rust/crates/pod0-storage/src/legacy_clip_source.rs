use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use pod0_domain::{
    ClipId, ClipRecord, ClipRevision, ClipSource, EpisodeId, PodcastId, SpeakerId,
    UnixTimestampMilliseconds, validate_clip,
};
use rusqlite::{Connection, OpenFlags};
use serde_json::from_slice;
use sha2::{Digest, Sha256};

use crate::backup::verify_connection;
use crate::clip_import_model::{ClipImportPlan, InspectedClipSource};
use crate::legacy_format::{RawAppState, timestamp_milliseconds, uuid_bytes};
use crate::{LegacySourceKind, StorageError};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CLIPS: usize = 100_000;

pub fn inspect_legacy_clip_source(path: &Path) -> Result<ClipImportPlan, StorageError> {
    inspect_clip_source(path).map(|source| source.plan)
}

pub(crate) fn inspect_clip_source(path: &Path) -> Result<InspectedClipSource, StorageError> {
    let mut file = File::open(path).map_err(|error| StorageError::io("open clip source", error))?;
    let mut header = [0_u8; 16];
    let read = file
        .read(&mut header)
        .map_err(|error| StorageError::io("read clip source header", error))?;
    file.rewind()
        .map_err(|error| StorageError::io("rewind clip source", error))?;
    let (kind, generation, raw) = if read == SQLITE_HEADER.len() && &header == SQLITE_HEADER {
        load_sqlite(path)?
    } else {
        load_json(file)?
    };
    transform(kind, generation, raw)
}

fn load_json(mut file: File) -> Result<(LegacySourceKind, u64, RawAppState), StorageError> {
    let size = file
        .metadata()
        .map_err(|error| StorageError::io("read clip JSON metadata", error))?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err(StorageError::ImportLimitExceeded {
            entity: "legacy clip JSON bytes",
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    file.read_to_end(&mut bytes)
        .map_err(|error| StorageError::io("read clip JSON", error))?;
    let raw: RawAppState = from_slice(&bytes).map_err(|_| StorageError::InvalidLegacyRecord {
        entity: "clips metadata",
        index: 0,
        detail: "legacy clips are not recognized JSON",
    })?;
    Ok((LegacySourceKind::LegacyJson, raw.generation, raw))
}

fn load_sqlite(path: &Path) -> Result<(LegacySourceKind, u64, RawAppState), StorageError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
    )
    .map_err(|error| StorageError::sqlite("open Swift clip source", error))?;
    connection
        .execute_batch("PRAGMA query_only=ON; PRAGMA foreign_keys=ON;")
        .map_err(|error| StorageError::sqlite("configure Swift clip source", error))?;
    verify_connection(&connection)?;
    let metadata: Vec<u8> = connection
        .query_row(
            "SELECT value FROM persistence_metadata WHERE key='app_state'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read Swift clip metadata", error))?;
    if metadata.len() as u64 > MAX_SOURCE_BYTES {
        return Err(StorageError::ImportLimitExceeded {
            entity: "legacy clip metadata bytes",
        });
    }
    let generation_text: String = connection
        .query_row(
            "SELECT CAST(value AS TEXT) FROM persistence_metadata WHERE key='generation'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| StorageError::sqlite("read Swift clip generation", error))?;
    let generation = generation_text
        .parse::<u64>()
        .map_err(|_| invalid(0, "clip source generation is invalid"))?;
    let mut raw: RawAppState =
        from_slice(&metadata).map_err(|_| StorageError::InvalidLegacyRecord {
            entity: "clips metadata",
            index: 0,
            detail: "Swift clips metadata is not recognized JSON",
        })?;
    if raw.generation != 0 && raw.generation != generation {
        return Err(invalid(0, "clip metadata and SQLite generations differ"));
    }
    raw.generation = generation;
    Ok((LegacySourceKind::SwiftSqlite, generation, raw))
}

fn transform(
    source_kind: LegacySourceKind,
    source_generation: u64,
    raw: RawAppState,
) -> Result<InspectedClipSource, StorageError> {
    if raw.clips.len() > MAX_CLIPS {
        return Err(StorageError::ImportLimitExceeded { entity: "clips" });
    }
    let mut identities = BTreeSet::new();
    let mut clips = Vec::with_capacity(raw.clips.len());
    for (offset, raw) in raw.clips.into_iter().enumerate() {
        let index = u32::try_from(offset)
            .map_err(|_| StorageError::ImportLimitExceeded { entity: "clips" })?;
        let clip_id = ClipId::from_bytes(uuid_bytes(&raw.id, "clip", index)?);
        if !identities.insert(clip_id) {
            return Err(invalid(index, "clip identity is duplicated"));
        }
        let episode_id = EpisodeId::from_bytes(uuid_bytes(&raw.episode_id, "clip episode", index)?);
        let podcast_id = PodcastId::from_bytes(uuid_bytes(&raw.podcast_id, "clip podcast", index)?);
        let start_milliseconds = u64::try_from(raw.start_milliseconds)
            .map_err(|_| invalid(index, "clip start is outside supported range"))?;
        let end_milliseconds = u64::try_from(raw.end_milliseconds)
            .map_err(|_| invalid(index, "clip end is outside supported range"))?;
        let source = match raw.source.as_deref().unwrap_or("touch") {
            "touch" => ClipSource::Touch,
            "auto" => ClipSource::Auto,
            "headphone" => ClipSource::Headphone,
            "carplay" => ClipSource::Carplay,
            "watch" => ClipSource::Watch,
            "siri" => ClipSource::Siri,
            "agent" => ClipSource::Agent,
            _ => return Err(invalid(index, "clip source is unsupported")),
        };
        validate_clip(
            start_milliseconds,
            end_milliseconds,
            raw.caption.as_deref(),
            &raw.frozen_transcript_text,
            source,
        )
        .map_err(|_| invalid(index, "clip fields are invalid"))?;
        let (speaker_id, speaker_label) = match raw.speaker_id.as_deref() {
            Some(value) => match uuid_bytes(value, "clip speaker", index) {
                Ok(bytes) => (Some(SpeakerId::from_bytes(bytes)), None),
                Err(_) => (None, Some(value.to_owned())),
            },
            None => (None, None),
        };
        clips.push(ClipRecord {
            clip_id,
            revision: ClipRevision::INITIAL,
            episode_id,
            podcast_id,
            start_milliseconds,
            end_milliseconds,
            created_at: UnixTimestampMilliseconds::new(timestamp_milliseconds(
                raw.created_at.as_ref(),
                "clip",
                index,
            )?),
            caption: raw.caption,
            speaker_id,
            speaker_label,
            frozen_transcript_text: raw.frozen_transcript_text,
            source,
            deleted: raw.deleted,
            evidence: None,
        });
    }
    clips.sort_by(|left, right| {
        right
            .created_at
            .value
            .cmp(&left.created_at.value)
            .then_with(|| left.clip_id.cmp(&right.clip_id))
    });
    let source_hash = digest(&clips);
    Ok(InspectedClipSource {
        plan: ClipImportPlan {
            source_kind,
            source_hash,
            source_generation,
            clip_count: u32::try_from(clips.len())
                .map_err(|_| StorageError::ImportLimitExceeded { entity: "clips" })?,
        },
        clips,
    })
}

pub(crate) fn digest(clips: &[ClipRecord]) -> String {
    let mut hash = Sha256::new();
    part(&mut hash, b"pod0-legacy-clips-v1");
    for clip in clips {
        part(&mut hash, &clip.clip_id.into_bytes());
        part(&mut hash, &clip.episode_id.into_bytes());
        part(&mut hash, &clip.podcast_id.into_bytes());
        hash.update(clip.start_milliseconds.to_be_bytes());
        hash.update(clip.end_milliseconds.to_be_bytes());
        hash.update(clip.created_at.value.to_be_bytes());
        optional(&mut hash, clip.caption.as_deref());
        match clip.speaker_id {
            Some(value) => {
                hash.update([1]);
                hash.update(value.into_bytes());
            }
            None => hash.update([0]),
        }
        optional(&mut hash, clip.speaker_label.as_deref());
        part(&mut hash, clip.frozen_transcript_text.as_bytes());
        hash.update([source_code(clip.source)]);
        hash.update([u8::from(clip.deleted)]);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn optional(hash: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash.update([1]);
            part(hash, value.as_bytes());
        }
        None => hash.update([0]),
    }
}

fn part(hash: &mut Sha256, value: &[u8]) {
    hash.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hash.update(value);
}

const fn source_code(source: ClipSource) -> u8 {
    match source {
        ClipSource::Touch => 1,
        ClipSource::Auto => 2,
        ClipSource::Headphone => 3,
        ClipSource::Carplay => 4,
        ClipSource::Watch => 5,
        ClipSource::Siri => 6,
        ClipSource::Agent => 7,
        ClipSource::Unsupported { .. } => 255,
    }
}

fn invalid(index: u32, detail: &'static str) -> StorageError {
    StorageError::InvalidLegacyRecord {
        entity: "clip",
        index,
        detail,
    }
}
