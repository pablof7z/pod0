use pod0_domain::{CommandId, TranscriptArtifact};
use rusqlite::{Transaction, params};

use crate::StorageError;
use crate::transcript_store_codec::sqlite_i64;

pub(crate) fn insert_artifact_rows(
    transaction: &Transaction<'_>,
    artifact: &TranscriptArtifact,
    source_import_id: Option<CommandId>,
    created_at_ms: i64,
) -> Result<(), StorageError> {
    let word_count = artifact
        .segments
        .iter()
        .try_fold(0_u64, |total, segment| {
            total.checked_add(segment.words.len() as u64)
        })
        .ok_or(StorageError::InvalidTranscriptArtifact)?;
    let artifact_id = artifact.artifact_id.into_bytes();
    let version_id = artifact.transcript_version_id.into_bytes();
    transaction
        .execute(
            "INSERT INTO pod0_transcript_artifacts(artifact_id,transcript_version_id,episode_id,\
             schema_version,integrity_digest,language,generated_at_ms,speaker_count,segment_count,\
             word_count,source_import_id,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                artifact_id.as_slice(), version_id.as_slice(),
                artifact.episode_id.into_bytes().as_slice(), artifact.schema_version,
                artifact.integrity_digest.into_bytes().as_slice(), artifact.language,
                artifact.generated_at.value, count(artifact.speakers.len())?,
                count(artifact.segments.len())?, sqlite_i64(word_count)?,
                source_import_id.map(|id| id.into_bytes().to_vec()), created_at_ms,
            ],
        )
        .map_err(|error| StorageError::sqlite("insert transcript artifact", error))?;
    insert_speakers(transaction, artifact)?;
    insert_segments_and_words(transaction, artifact)
}

fn insert_speakers(
    transaction: &Transaction<'_>,
    artifact: &TranscriptArtifact,
) -> Result<(), StorageError> {
    let artifact_id = artifact.artifact_id.into_bytes();
    let mut statement = transaction
        .prepare(
            "INSERT INTO pod0_transcript_speakers(artifact_id,ordinal,speaker_id,label,display_name) \
             VALUES(?1,?2,?3,?4,?5)",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript speakers", error))?;
    for (ordinal, speaker) in artifact.speakers.iter().enumerate() {
        statement
            .execute(params![
                artifact_id.as_slice(),
                count(ordinal)?,
                speaker.speaker_id.into_bytes().as_slice(),
                speaker.label,
                speaker.display_name
            ])
            .map_err(|error| StorageError::sqlite("insert transcript speaker", error))?;
    }
    Ok(())
}

fn insert_segments_and_words(
    transaction: &Transaction<'_>,
    artifact: &TranscriptArtifact,
) -> Result<(), StorageError> {
    let artifact_id = artifact.artifact_id.into_bytes();
    let version_id = artifact.transcript_version_id.into_bytes();
    for segment in &artifact.segments {
        let segment_id = segment.segment_id.into_bytes();
        transaction
            .execute(
                "INSERT INTO pod0_transcript_artifact_segments(artifact_id,transcript_version_id,\
                 segment_id,ordinal,raw_text,word_count) VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    artifact_id.as_slice(),
                    version_id.as_slice(),
                    segment_id.as_slice(),
                    segment.ordinal,
                    segment.text,
                    count(segment.words.len())?
                ],
            )
            .map_err(|error| StorageError::sqlite("insert transcript artifact segment", error))?;
        insert_words(
            transaction,
            artifact_id.as_slice(),
            version_id.as_slice(),
            segment_id.as_slice(),
            &segment.words,
        )?;
    }
    Ok(())
}

fn insert_words(
    transaction: &Transaction<'_>,
    artifact_id: &[u8],
    version_id: &[u8],
    segment_id: &[u8],
    words: &[pod0_domain::TranscriptArtifactWordInput],
) -> Result<(), StorageError> {
    let mut statement = transaction
        .prepare(
            "INSERT INTO pod0_transcript_words(artifact_id,transcript_version_id,segment_id,\
             ordinal,text,start_ms,end_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript words", error))?;
    for (ordinal, word) in words.iter().enumerate() {
        statement
            .execute(params![
                artifact_id,
                version_id,
                segment_id,
                count(ordinal)?,
                word.text,
                sqlite_i64(word.start_milliseconds)?,
                sqlite_i64(word.end_milliseconds)?
            ])
            .map_err(|error| StorageError::sqlite("insert transcript word", error))?;
    }
    Ok(())
}

fn count(value: usize) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::InvalidTranscriptArtifact)
}
