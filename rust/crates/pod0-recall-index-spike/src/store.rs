use std::collections::BTreeSet;

use pod0_application::RecallEmbeddingVector;
use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId, PodcastId};
use rusqlite::{Transaction, params};

use crate::{RecallCancellation, RecallIndexError, RecallIndexSpan, RecallIndexSpike};

impl RecallIndexSpike {
    pub fn rebuild_episode(
        &mut self,
        spans: &[RecallIndexSpan],
        cancellation: &RecallCancellation,
    ) -> Result<u32, RecallIndexError> {
        let first = spans.first().ok_or(RecallIndexError::InvalidInput(
            "recall index generation must not be empty",
        ))?;
        if spans.iter().any(|span| {
            span.episode_id != first.episode_id
                || span.generation_id != first.generation_id
                || span.embedding.values.len() != self.dimensions
                || span.text.is_empty()
        }) || spans
            .iter()
            .map(|span| span.span_id)
            .collect::<BTreeSet<_>>()
            .len()
            != spans.len()
        {
            return Err(RecallIndexError::InvalidInput(
                "recall index generation is inconsistent",
            ));
        }
        check_cancelled(cancellation)?;
        let episode_key = episode_key(first.episode_id);
        let transaction = self.connection.transaction()?;
        if let Err(error) = replace_episode(&transaction, &episode_key, spans, cancellation) {
            return if cancellation.is_cancelled() {
                Err(RecallIndexError::Cancelled)
            } else {
                Err(error)
            };
        }
        transaction.commit()?;
        u32::try_from(spans.len())
            .map_err(|_| RecallIndexError::InvalidInput("recall index generation exceeds UInt32"))
    }

    pub fn stored_span_count(&self) -> Result<u64, RecallIndexError> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM recall_spike_meta", [], |row| {
                    row.get(0)
                })?;
        u64::try_from(count)
            .map_err(|_| RecallIndexError::InvalidInput("recall index count is invalid"))
    }

    pub fn optimize(&self) -> Result<(), RecallIndexError> {
        self.connection.execute_batch("PRAGMA optimize;")?;
        Ok(())
    }
}

fn replace_episode(
    transaction: &Transaction<'_>,
    episode_key: &str,
    spans: &[RecallIndexSpan],
    cancellation: &RecallCancellation,
) -> Result<(), RecallIndexError> {
    let existing = {
        let mut statement =
            transaction.prepare("SELECT span_id FROM recall_spike_meta WHERE episode_id=?1")?;
        statement
            .query_map([episode_key], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for span_id in existing {
        transaction.execute("DELETE FROM recall_spike_vec WHERE span_id=?1", [&span_id])?;
        transaction.execute("DELETE FROM recall_spike_fts WHERE span_id=?1", [&span_id])?;
    }
    transaction.execute(
        "DELETE FROM recall_spike_meta WHERE episode_id=?1",
        [episode_key],
    )?;
    let mut metadata = transaction.prepare(
        "INSERT INTO recall_spike_meta(
           span_id,generation_id,episode_id,podcast_id,text
         ) VALUES(?1,?2,?3,?4,?5)",
    )?;
    let mut vector = transaction.prepare(
        "INSERT INTO recall_spike_vec(span_id,episode_id,podcast_id,embedding)
         VALUES(?1,?2,?3,?4)",
    )?;
    let mut lexical = transaction.prepare(
        "INSERT INTO recall_spike_fts(span_id,episode_id,podcast_id,text)
         VALUES(?1,?2,?3,?4)",
    )?;
    for (offset, span) in spans.iter().enumerate() {
        if offset % 16 == 0 {
            check_cancelled(cancellation)?;
        }
        let span_id = span_key(span.span_id);
        let generation_id = generation_key(span.generation_id);
        let episode_id = episode_key_owned(span.episode_id);
        let podcast_id = podcast_key(span.podcast_id);
        metadata.execute(params![
            &span_id,
            &generation_id,
            &episode_id,
            &podcast_id,
            &span.text
        ])?;
        vector.execute(params![
            &span_id,
            &episode_id,
            &podcast_id,
            embedding_blob(&span.embedding)
        ])?;
        lexical.execute(params![&span_id, &episode_id, &podcast_id, &span.text])?;
    }
    check_cancelled(cancellation)
}

pub(crate) fn check_cancelled(cancellation: &RecallCancellation) -> Result<(), RecallIndexError> {
    if cancellation.is_cancelled() {
        Err(RecallIndexError::Cancelled)
    } else {
        Ok(())
    }
}

pub(crate) fn embedding_blob(embedding: &RecallEmbeddingVector) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.values.len() * size_of::<f32>());
    for value in &embedding.values {
        bytes.extend_from_slice(&((*value as f32) / 1_000_000.0).to_le_bytes());
    }
    bytes
}

pub(crate) fn span_key(value: EvidenceSpanId) -> String {
    stable_key(value.high, value.low)
}

pub(crate) fn generation_key(value: EvidenceGenerationId) -> String {
    stable_key(value.high, value.low)
}

pub(crate) fn episode_key(value: EpisodeId) -> String {
    stable_key(value.high, value.low)
}

fn episode_key_owned(value: EpisodeId) -> String {
    episode_key(value)
}

pub(crate) fn podcast_key(value: PodcastId) -> String {
    stable_key(value.high, value.low)
}

pub(crate) fn parse_span_key(value: &str) -> Option<EvidenceSpanId> {
    parse_key(value).map(|(high, low)| EvidenceSpanId::from_parts(high, low))
}

pub(crate) fn parse_generation_key(value: &str) -> Option<EvidenceGenerationId> {
    parse_key(value).map(|(high, low)| EvidenceGenerationId::from_parts(high, low))
}

pub(crate) fn parse_episode_key(value: &str) -> Option<EpisodeId> {
    parse_key(value).map(|(high, low)| EpisodeId::from_parts(high, low))
}

fn stable_key(high: u64, low: u64) -> String {
    format!("{high:016x}{low:016x}")
}

fn parse_key(value: &str) -> Option<(u64, u64)> {
    (value.len() == 32).then_some(())?;
    Some((
        u64::from_str_radix(&value[..16], 16).ok()?,
        u64::from_str_radix(&value[16..], 16).ok()?,
    ))
}
