use std::collections::{BTreeMap, BTreeSet};

use pod0_application::RecallEmbeddingVector;
use pod0_domain::EvidenceSpanId;
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::identity::{episode_key, generation_key, podcast_key, span_key};
use crate::store::check_cancelled;
use crate::{
    RecallCancellation, RecallIndex, RecallIndexError, RecallIndexSpan, RecallSpanEmbedding,
};

impl RecallIndex {
    pub fn cache_embeddings(
        &mut self,
        spans: &[RecallIndexSpan],
        observations: &[RecallSpanEmbedding],
        cancellation: &RecallCancellation,
    ) -> Result<(), RecallIndexError> {
        validate_batch(spans, observations, self.dimensions)?;
        check_cancelled(cancellation)?;
        let by_id = observations
            .iter()
            .map(|observation| (observation.span_id, &observation.embedding))
            .collect::<BTreeMap<_, _>>();
        let transaction = self.connection.transaction()?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO pod0_recall_embedding_cache_v1(
                   span_id,generation_id,episode_id,podcast_id,
                   text_digest,dimensions,embedding
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(span_id,generation_id) DO UPDATE SET
                   episode_id=excluded.episode_id,
                   podcast_id=excluded.podcast_id,
                   text_digest=excluded.text_digest,
                   dimensions=excluded.dimensions,
                   embedding=excluded.embedding",
            )?;
            for span in spans {
                check_cancelled(cancellation)?;
                let embedding = by_id[&span.span_id];
                statement.execute(params![
                    span_key(span.span_id),
                    generation_key(span.generation_id),
                    episode_key(span.episode_id),
                    podcast_key(span.podcast_id),
                    text_digest(&span.text).as_slice(),
                    u32::try_from(self.dimensions).expect("bounded dimensions"),
                    quantized_embedding_blob(embedding),
                ])?;
            }
        }
        check_cancelled(cancellation)?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn cached_embedding(
        &self,
        span: &RecallIndexSpan,
    ) -> Result<Option<RecallEmbeddingVector>, RecallIndexError> {
        let row = self
            .connection
            .query_row(
                "SELECT text_digest,dimensions,embedding
                 FROM pod0_recall_embedding_cache_v1
                 WHERE span_id=?1 AND generation_id=?2",
                params![span_key(span.span_id), generation_key(span.generation_id)],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((digest, dimensions, bytes)) = row else {
            return Ok(None);
        };
        if digest != text_digest(&span.text)
            || usize::try_from(dimensions).ok() != Some(self.dimensions)
        {
            return Ok(None);
        }
        decode_quantized_embedding(&bytes, self.dimensions).map(Some)
    }
}

fn validate_batch(
    spans: &[RecallIndexSpan],
    observations: &[RecallSpanEmbedding],
    dimensions: usize,
) -> Result<(), RecallIndexError> {
    if spans.is_empty() || spans.len() != observations.len() {
        return Err(RecallIndexError::InvalidInput(
            "recall embedding batch count is invalid",
        ));
    }
    let expected = spans
        .iter()
        .map(|span| span.span_id)
        .collect::<BTreeSet<EvidenceSpanId>>();
    let observed = observations
        .iter()
        .map(|observation| observation.span_id)
        .collect::<BTreeSet<EvidenceSpanId>>();
    if expected.len() != spans.len()
        || observed.len() != observations.len()
        || expected != observed
        || observations
            .iter()
            .any(|observation| observation.embedding.values.len() != dimensions)
    {
        return Err(RecallIndexError::InvalidInput(
            "recall embedding batch violates the typed contract",
        ));
    }
    Ok(())
}

pub(crate) fn vector_embedding_blob(embedding: &RecallEmbeddingVector) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.values.len() * size_of::<f32>());
    for value in &embedding.values {
        bytes.extend_from_slice(&((*value as f32) / 1_000_000.0).to_le_bytes());
    }
    bytes
}

fn quantized_embedding_blob(embedding: &RecallEmbeddingVector) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.values.len() * size_of::<i32>());
    for value in &embedding.values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode_quantized_embedding(
    bytes: &[u8],
    dimensions: usize,
) -> Result<RecallEmbeddingVector, RecallIndexError> {
    if bytes.len() != dimensions.saturating_mul(size_of::<i32>()) {
        return Err(RecallIndexError::InvalidInput(
            "cached recall embedding has invalid dimensions",
        ));
    }
    let values = bytes
        .chunks_exact(size_of::<i32>())
        .map(|chunk| i32::from_le_bytes(chunk.try_into().expect("exact chunk")))
        .collect();
    Ok(RecallEmbeddingVector { values })
}

fn text_digest(text: &str) -> Vec<u8> {
    Sha256::digest(text.as_bytes()).to_vec()
}
