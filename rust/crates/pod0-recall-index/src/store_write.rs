use pod0_application::RecallEmbeddingVector;
use rusqlite::{Transaction, params};

use crate::cache::vector_embedding_blob;
use crate::identity::{episode_key, generation_key, podcast_key, span_key};
use crate::store::check_cancelled;
use crate::{MAX_RECALL_EMBEDDING_BATCH, RecallCancellation, RecallIndexError, RecallIndexSpan};

pub(crate) fn insert_episode(
    transaction: &Transaction<'_>,
    spans: &[RecallIndexSpan],
    embeddings: &[RecallEmbeddingVector],
    dimensions: usize,
    cancellation: &RecallCancellation,
) -> Result<(), RecallIndexError> {
    let first = &spans[0];
    let mut metadata = transaction.prepare(
        "INSERT INTO pod0_recall_meta_v1(
           span_id,generation_id,episode_id,podcast_id,text
         ) VALUES(?1,?2,?3,?4,?5)",
    )?;
    let mut vector = transaction.prepare(
        "INSERT INTO pod0_recall_vec_v1(span_id,episode_id,podcast_id,embedding)
         VALUES(?1,?2,?3,?4)",
    )?;
    let mut lexical = transaction.prepare(
        "INSERT INTO pod0_recall_fts_v1(span_id,episode_id,podcast_id,text)
         VALUES(?1,?2,?3,?4)",
    )?;
    for (offset, (span, embedding)) in spans.iter().zip(embeddings).enumerate() {
        if offset % MAX_RECALL_EMBEDDING_BATCH == 0 {
            check_cancelled(cancellation)?;
        }
        if embedding.values.len() != dimensions {
            return Err(RecallIndexError::InvalidInput(
                "cached recall embedding has invalid dimensions",
            ));
        }
        let span_id = span_key(span.span_id);
        let episode_id = episode_key(span.episode_id);
        let podcast_id = podcast_key(span.podcast_id);
        metadata.execute(params![
            span_id,
            generation_key(span.generation_id),
            episode_id,
            podcast_id,
            span.text,
        ])?;
        vector.execute(params![
            span_id,
            episode_id,
            podcast_id,
            vector_embedding_blob(embedding)
        ])?;
        lexical.execute(params![span_id, episode_id, podcast_id, span.text])?;
    }
    check_cancelled(cancellation)?;
    transaction.execute(
        "INSERT INTO pod0_recall_generations_v1(
           episode_id,generation_id,podcast_id,span_count
         ) VALUES(?1,?2,?3,?4)",
        params![
            episode_key(first.episode_id),
            generation_key(first.generation_id),
            podcast_key(first.podcast_id),
            u32::try_from(spans.len()).map_err(|_| RecallIndexError::InvalidInput(
                "recall index generation exceeds UInt32"
            ))?,
        ],
    )?;
    Ok(())
}

pub(crate) fn delete_episode_execution(
    transaction: &Transaction<'_>,
    episode: &str,
) -> Result<(), RecallIndexError> {
    let existing = {
        let mut statement =
            transaction.prepare("SELECT span_id FROM pod0_recall_meta_v1 WHERE episode_id=?1")?;
        statement
            .query_map([episode], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for span_id in existing {
        transaction.execute(
            "DELETE FROM pod0_recall_vec_v1 WHERE span_id=?1",
            [&span_id],
        )?;
        transaction.execute(
            "DELETE FROM pod0_recall_fts_v1 WHERE span_id=?1",
            [&span_id],
        )?;
    }
    transaction.execute(
        "DELETE FROM pod0_recall_meta_v1 WHERE episode_id=?1",
        [episode],
    )?;
    transaction.execute(
        "DELETE FROM pod0_recall_generations_v1 WHERE episode_id=?1",
        [episode],
    )?;
    Ok(())
}
