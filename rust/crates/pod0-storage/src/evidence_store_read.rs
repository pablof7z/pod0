use pod0_domain::{
    ContentDigest, EpisodeId, EvidenceChunkPolicy, EvidenceGenerationId, EvidenceSpan, PodcastId,
    TranscriptEvidenceArtifact, TranscriptProvenance, TranscriptSegmentRecord,
    TranscriptVersionRecord,
};
use rusqlite::{Connection, OptionalExtension};

use crate::evidence_codec::{
    artifact_error, decode_source, digest, episode_id, generation_id, optional_speaker_id,
    podcast_id, segment_id, span_id, stored_u32, stored_u64, version_id,
};
use crate::{EvidenceGenerationState, EvidenceGenerationSummary, StorageError};

struct GenerationRow {
    generation_id: EvidenceGenerationId,
    transcript_version_id: pod0_domain::TranscriptVersionId,
    episode_id: EpisodeId,
    podcast_id: PodcastId,
    artifact_schema_version: u32,
    integrity_digest: ContentDigest,
    policy: EvidenceChunkPolicy,
    segment_count: u32,
    span_count: u32,
    state: EvidenceGenerationState,
    staged_at_ms: i64,
    verified_at_ms: Option<i64>,
    source_revision: String,
    content_digest: ContentDigest,
    provenance: TranscriptProvenance,
}

pub(crate) fn read_summary(
    connection: &Connection,
    generation: EvidenceGenerationId,
) -> Result<Option<EvidenceGenerationSummary>, StorageError> {
    Ok(generation_row(connection, generation)?.map(|row| summary(&row)))
}

pub(crate) fn read_artifact(
    connection: &Connection,
    generation: EvidenceGenerationId,
) -> Result<Option<TranscriptEvidenceArtifact>, StorageError> {
    let Some(row) = generation_row(connection, generation)? else {
        return Ok(None);
    };
    let segments = read_segments(connection, &row)?;
    let spans = read_spans(connection, &row)?;
    if usize::try_from(row.segment_count).ok() != Some(segments.len())
        || usize::try_from(row.span_count).ok() != Some(spans.len())
    {
        return Err(StorageError::InvalidEvidenceArtifact);
    }
    let artifact = TranscriptEvidenceArtifact {
        schema_version: row.artifact_schema_version,
        generation_id: row.generation_id,
        integrity_digest: row.integrity_digest,
        policy: row.policy,
        version: TranscriptVersionRecord {
            transcript_version_id: row.transcript_version_id,
            episode_id: row.episode_id,
            podcast_id: row.podcast_id,
            source_revision: row.source_revision,
            content_digest: row.content_digest,
            provenance: row.provenance,
        },
        segments,
        spans,
    };
    artifact.verify_integrity().map_err(artifact_error)?;
    Ok(Some(artifact))
}

pub(crate) fn selected_generation_id(
    connection: &Connection,
    episode: EpisodeId,
) -> Result<Option<EvidenceGenerationId>, StorageError> {
    let bytes = connection
        .query_row(
            "SELECT generation_id FROM pod0_evidence_selection WHERE episode_id=?1",
            [episode.into_bytes().as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read selected evidence generation", error))?;
    bytes.as_deref().map(generation_id).transpose()
}

fn generation_row(
    connection: &Connection,
    generation: EvidenceGenerationId,
) -> Result<Option<GenerationRow>, StorageError> {
    connection
        .query_row(
            "SELECT g.generation_id,g.transcript_version_id,g.episode_id,d.podcast_id,\
             g.artifact_schema_version,g.integrity_digest,g.chunk_policy_version,g.target_tokens,\
             g.overlap_per_mille,g.snap_tolerance_per_mille,d.segment_count,g.span_count,g.state,\
             g.staged_at_ms,g.verified_at_ms,d.source_revision,d.content_digest,d.source_code,\
             d.source_wire_code,d.provider,d.source_payload_digest \
             FROM pod0_evidence_generations g JOIN pod0_transcript_documents d \
             ON d.transcript_version_id=g.transcript_version_id WHERE g.generation_id=?1",
            [generation.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, Option<i64>>(14)?,
                    row.get::<_, String>(15)?,
                    row.get::<_, Vec<u8>>(16)?,
                    row.get::<_, i64>(17)?,
                    row.get::<_, Option<i64>>(18)?,
                    row.get::<_, Option<String>>(19)?,
                    row.get::<_, Vec<u8>>(20)?,
                ))
            },
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read evidence generation", error))?
        .map(decode_generation_row)
        .transpose()
}

type RawGenerationRow = (
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    i64,
    Vec<u8>,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    String,
    i64,
    Option<i64>,
    String,
    Vec<u8>,
    i64,
    Option<i64>,
    Option<String>,
    Vec<u8>,
);

fn decode_generation_row(value: RawGenerationRow) -> Result<GenerationRow, StorageError> {
    let state = match value.12.as_str() {
        "staged" => EvidenceGenerationState::Staged,
        "verified" => EvidenceGenerationState::Verified,
        _ => return Err(StorageError::InvalidEvidenceArtifact),
    };
    Ok(GenerationRow {
        generation_id: generation_id(&value.0)?,
        transcript_version_id: version_id(&value.1)?,
        episode_id: episode_id(&value.2)?,
        podcast_id: podcast_id(&value.3)?,
        artifact_schema_version: stored_u32(value.4, "artifact schema")?,
        integrity_digest: digest(&value.5)?,
        policy: EvidenceChunkPolicy {
            version: stored_u32(value.6, "chunk policy")?,
            target_tokens: u16::try_from(value.7)
                .map_err(|_| StorageError::InvalidEvidenceArtifact)?,
            overlap_per_mille: u16::try_from(value.8)
                .map_err(|_| StorageError::InvalidEvidenceArtifact)?,
            snap_tolerance_per_mille: u16::try_from(value.9)
                .map_err(|_| StorageError::InvalidEvidenceArtifact)?,
        },
        segment_count: stored_u32(value.10, "segment count")?,
        span_count: stored_u32(value.11, "span count")?,
        state,
        staged_at_ms: value.13,
        verified_at_ms: value.14,
        source_revision: value.15,
        content_digest: digest(&value.16)?,
        provenance: TranscriptProvenance {
            source: decode_source(value.17, value.18)?,
            provider: value.19,
            source_payload_digest: digest(&value.20)?,
        },
    })
}

fn summary(row: &GenerationRow) -> EvidenceGenerationSummary {
    EvidenceGenerationSummary {
        generation_id: row.generation_id,
        transcript_version_id: row.transcript_version_id,
        episode_id: row.episode_id,
        artifact_schema_version: row.artifact_schema_version,
        policy: row.policy,
        segment_count: row.segment_count,
        span_count: row.span_count,
        state: row.state,
        staged_at_ms: row.staged_at_ms,
        verified_at_ms: row.verified_at_ms,
    }
}

fn read_segments(
    connection: &Connection,
    generation: &GenerationRow,
) -> Result<Vec<TranscriptSegmentRecord>, StorageError> {
    let mut statement = connection
        .prepare(
            "SELECT segment_id,ordinal,text,start_ms,end_ms,speaker_id FROM \
                  pod0_transcript_segments WHERE transcript_version_id=?1 ORDER BY ordinal",
        )
        .map_err(|error| StorageError::sqlite("prepare transcript segments", error))?;
    statement
        .query_map(
            [generation.transcript_version_id.into_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                ))
            },
        )
        .map_err(|error| StorageError::sqlite("read transcript segments", error))?
        .map(|row| {
            let (id, ordinal, text, start, end, speaker) =
                row.map_err(|error| StorageError::sqlite("decode transcript segment", error))?;
            Ok(TranscriptSegmentRecord {
                segment_id: segment_id(&id)?,
                ordinal: stored_u32(ordinal, "segment ordinal")?,
                text,
                start_milliseconds: stored_u64(start, "segment start")?,
                end_milliseconds: stored_u64(end, "segment end")?,
                speaker_id: optional_speaker_id(speaker)?,
            })
        })
        .collect()
}

fn read_spans(
    connection: &Connection,
    generation: &GenerationRow,
) -> Result<Vec<EvidenceSpan>, StorageError> {
    let mut statement = connection
        .prepare("SELECT span_id,first_segment_id,last_segment_id,start_segment_ordinal,\
                  end_segment_ordinal_exclusive,start_ms,end_ms,text,speaker_id,chunk_policy_version \
                  FROM pod0_evidence_spans WHERE generation_id=?1 ORDER BY sort_order")
        .map_err(|error| StorageError::sqlite("prepare evidence spans", error))?;
    statement
        .query_map([generation.generation_id.into_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<Vec<u8>>>(8)?,
                row.get::<_, i64>(9)?,
            ))
        })
        .map_err(|error| StorageError::sqlite("read evidence spans", error))?
        .map(|row| {
            let value = row.map_err(|error| StorageError::sqlite("decode evidence span", error))?;
            Ok(EvidenceSpan {
                span_id: span_id(&value.0)?,
                transcript_version_id: generation.transcript_version_id,
                transcript_content_digest: generation.content_digest,
                episode_id: generation.episode_id,
                podcast_id: generation.podcast_id,
                first_segment_id: segment_id(&value.1)?,
                last_segment_id: segment_id(&value.2)?,
                start_segment_ordinal: stored_u32(value.3, "span start ordinal")?,
                end_segment_ordinal_exclusive: stored_u32(value.4, "span end ordinal")?,
                start_milliseconds: stored_u64(value.5, "span start")?,
                end_milliseconds: stored_u64(value.6, "span end")?,
                text: value.7,
                speaker_id: optional_speaker_id(value.8)?,
                provenance: generation.provenance.clone(),
                chunk_policy_version: stored_u32(value.9, "span policy")?,
            })
        })
        .collect()
}
