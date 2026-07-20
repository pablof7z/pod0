use std::collections::BTreeSet;

use pod0_application::{RecallEmbeddingVector, RecallScope};
use pod0_domain::{EpisodeId, EvidenceGenerationId, EvidenceSpanId};
use rusqlite::{OptionalExtension, params};

use crate::identity::{episode_key, generation_key, podcast_key, span_key};
use crate::store_write::{delete_episode_execution, insert_episode};
use crate::{
    MAX_RECALL_EMBEDDING_BATCH, RecallCancellation, RecallEmbeddingRequest, RecallIndex,
    RecallIndexError, RecallIndexPlan, RecallIndexSpan,
};

impl RecallIndex {
    pub fn prepare_episode(
        &mut self,
        spans: &[RecallIndexSpan],
        cancellation: &RecallCancellation,
    ) -> Result<RecallIndexPlan, RecallIndexError> {
        validate_generation(spans, self.dimensions)?;
        check_cancelled(cancellation)?;
        if self.generation_matches(spans)? {
            return ready_plan(spans.len());
        }
        self.clear_stale_episode(spans[0].episode_id, spans[0].generation_id)?;
        let missing = spans
            .iter()
            .filter_map(|span| match self.cached_embedding(span) {
                Ok(Some(_)) => None,
                Ok(None) => Some(Ok(RecallEmbeddingRequest {
                    span_id: span.span_id,
                    text: span.text.clone(),
                })),
                Err(error) => Some(Err(error)),
            })
            .take(MAX_RECALL_EMBEDDING_BATCH)
            .collect::<Result<Vec<_>, _>>()?;
        if !missing.is_empty() {
            return Ok(RecallIndexPlan::NeedsEmbeddings { spans: missing });
        }
        let embeddings = spans
            .iter()
            .map(|span| {
                self.cached_embedding(span)?
                    .ok_or(RecallIndexError::InvalidInput(
                        "recall embedding cache became incomplete",
                    ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.replace_episode(spans, &embeddings, cancellation)?;
        ready_plan(spans.len())
    }

    pub fn has_ready_scope(&self, scope: RecallScope) -> Result<bool, RecallIndexError> {
        let (sql, value) = match scope {
            RecallScope::Library => ("SELECT 1 FROM pod0_recall_generations_v1 LIMIT 1", None),
            RecallScope::Podcast { podcast_id } => (
                "SELECT 1 FROM pod0_recall_generations_v1 WHERE podcast_id=?1 LIMIT 1",
                Some(podcast_key(podcast_id)),
            ),
            RecallScope::Episode { episode_id } => (
                "SELECT 1 FROM pod0_recall_generations_v1 WHERE episode_id=?1 LIMIT 1",
                Some(episode_key(episode_id)),
            ),
            RecallScope::Unsupported { .. } => return Ok(false),
        };
        let found = match value {
            Some(value) => self
                .connection
                .query_row(sql, [value], |_| Ok(()))
                .optional()?,
            None => self.connection.query_row(sql, [], |_| Ok(())).optional()?,
        };
        Ok(found.is_some())
    }

    pub fn stored_span_count(&self) -> Result<u64, RecallIndexError> {
        let count: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM pod0_recall_meta_v1", [], |row| {
                    row.get(0)
                })?;
        u64::try_from(count)
            .map_err(|_| RecallIndexError::InvalidInput("recall index count is invalid"))
    }

    pub fn optimize(&self) -> Result<(), RecallIndexError> {
        self.connection.execute_batch("PRAGMA optimize;")?;
        Ok(())
    }

    fn generation_matches(&self, spans: &[RecallIndexSpan]) -> Result<bool, RecallIndexError> {
        let first = &spans[0];
        let generation = self
            .connection
            .query_row(
                "SELECT generation_id,span_count FROM pod0_recall_generations_v1
                 WHERE episode_id=?1",
                [episode_key(first.episode_id)],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)),
            )
            .optional()?;
        if generation
            != Some((
                generation_key(first.generation_id),
                u32::try_from(spans.len()).unwrap_or(u32::MAX),
            ))
        {
            return Ok(false);
        }
        let mut statement = self.connection.prepare(
            "SELECT m.span_id,
               CASE WHEN v.span_id IS NULL THEN 0 ELSE 1 END,
               CASE WHEN f.span_id IS NULL THEN 0 ELSE 1 END
             FROM pod0_recall_meta_v1 m
             LEFT JOIN pod0_recall_vec_v1 v ON v.span_id=m.span_id
             LEFT JOIN pod0_recall_fts_v1 f ON f.span_id=m.span_id
             WHERE m.episode_id=?1 AND m.generation_id=?2",
        )?;
        let rows = statement
            .query_map(
                params![
                    episode_key(first.episode_id),
                    generation_key(first.generation_id)
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let expected = spans
            .iter()
            .map(|span| span_key(span.span_id))
            .collect::<BTreeSet<_>>();
        Ok(rows.len() == spans.len()
            && rows
                .iter()
                .all(|(_, vector, lexical)| *vector == 1 && *lexical == 1)
            && rows
                .into_iter()
                .map(|(id, _, _)| id)
                .collect::<BTreeSet<_>>()
                == expected)
    }

    fn clear_stale_episode(
        &mut self,
        episode_id: EpisodeId,
        generation_id: EvidenceGenerationId,
    ) -> Result<(), RecallIndexError> {
        let episode = episode_key(episode_id);
        let current = self
            .connection
            .query_row(
                "SELECT generation_id FROM pod0_recall_generations_v1 WHERE episode_id=?1",
                [&episode],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if current.as_deref() == Some(&generation_key(generation_id)) {
            return Ok(());
        }
        let transaction = self.connection.transaction()?;
        delete_episode_execution(&transaction, &episode)?;
        transaction.commit()?;
        Ok(())
    }

    fn replace_episode(
        &mut self,
        spans: &[RecallIndexSpan],
        embeddings: &[RecallEmbeddingVector],
        cancellation: &RecallCancellation,
    ) -> Result<(), RecallIndexError> {
        check_cancelled(cancellation)?;
        let first = &spans[0];
        let episode = episode_key(first.episode_id);
        let transaction = self.connection.transaction()?;
        delete_episode_execution(&transaction, &episode)?;
        if let Err(error) = insert_episode(
            &transaction,
            spans,
            embeddings,
            self.dimensions,
            cancellation,
        ) {
            return if cancellation.is_cancelled() {
                Err(RecallIndexError::Cancelled)
            } else {
                Err(error)
            };
        }
        transaction.commit()?;
        self.connection.execute(
            "DELETE FROM pod0_recall_embedding_cache_v1
             WHERE episode_id=?1 AND generation_id<>?2",
            params![episode, generation_key(first.generation_id)],
        )?;
        Ok(())
    }
}

fn validate_generation(
    spans: &[RecallIndexSpan],
    dimensions: usize,
) -> Result<(), RecallIndexError> {
    let first = spans.first().ok_or(RecallIndexError::InvalidInput(
        "recall index generation must not be empty",
    ))?;
    if dimensions == 0
        || spans.iter().any(|span| {
            span.episode_id != first.episode_id
                || span.generation_id != first.generation_id
                || span.podcast_id != first.podcast_id
                || span.text.is_empty()
        })
        || spans
            .iter()
            .map(|span| span.span_id)
            .collect::<BTreeSet<EvidenceSpanId>>()
            .len()
            != spans.len()
    {
        return Err(RecallIndexError::InvalidInput(
            "recall index generation is inconsistent",
        ));
    }
    Ok(())
}

fn ready_plan(count: usize) -> Result<RecallIndexPlan, RecallIndexError> {
    Ok(RecallIndexPlan::Ready {
        indexed_span_count: u32::try_from(count).map_err(|_| {
            RecallIndexError::InvalidInput("recall index generation exceeds UInt32")
        })?,
    })
}

pub(crate) fn check_cancelled(cancellation: &RecallCancellation) -> Result<(), RecallIndexError> {
    if cancellation.is_cancelled() {
        Err(RecallIndexError::Cancelled)
    } else {
        Ok(())
    }
}
