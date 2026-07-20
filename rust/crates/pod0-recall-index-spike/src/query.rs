use std::collections::BTreeMap;

use pod0_application::{RecallCandidateObservation, RecallEmbeddingVector, RecallScope};
use rusqlite::{OptionalExtension, params, params_from_iter};

use crate::store::{
    check_cancelled, embedding_blob, episode_key, parse_episode_key, parse_generation_key,
    parse_span_key, podcast_key,
};
use crate::{RecallCancellation, RecallIndexError, RecallIndexSpike};

impl RecallIndexSpike {
    #[allow(clippy::too_many_arguments)]
    pub fn retrieve(
        &self,
        query_embedding: &RecallEmbeddingVector,
        lexical_query: &str,
        scope: RecallScope,
        maximum_vector_candidates: u16,
        maximum_lexical_candidates: u16,
        maximum_total_candidates: u16,
        cancellation: &RecallCancellation,
    ) -> Result<Vec<RecallCandidateObservation>, RecallIndexError> {
        if query_embedding.values.len() != self.dimensions
            || usize::from(maximum_vector_candidates) + usize::from(maximum_lexical_candidates)
                > usize::from(maximum_total_candidates)
        {
            return Err(RecallIndexError::InvalidInput(
                "recall candidate request violates bounds",
            ));
        }
        check_cancelled(cancellation)?;
        let filter = ScopeFilter::new(scope)?;
        let vector = self.vector_rows(
            query_embedding,
            &filter,
            maximum_vector_candidates,
            cancellation,
        )?;
        check_cancelled(cancellation)?;
        let lexical = self.lexical_rows(
            lexical_query,
            &filter,
            maximum_lexical_candidates,
            cancellation,
        )?;
        check_cancelled(cancellation)?;
        combine_candidates(
            &self.connection,
            &vector,
            &lexical,
            maximum_total_candidates,
        )
    }

    fn vector_rows(
        &self,
        query: &RecallEmbeddingVector,
        filter: &ScopeFilter,
        limit: u16,
        cancellation: &RecallCancellation,
    ) -> Result<Vec<String>, RecallIndexError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT span_id FROM recall_spike_vec
             WHERE embedding MATCH ?1{}
             ORDER BY distance LIMIT ?2",
            filter.sql
        );
        let mut values = vec![
            rusqlite::types::Value::Blob(embedding_blob(query)),
            rusqlite::types::Value::Integer(i64::from(limit)),
        ];
        values.extend(filter.values.clone());
        let mut statement = self.connection.prepare(&sql)?;
        let result = statement
            .query_map(params_from_iter(values), |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into);
        map_cancellation(result, cancellation)
    }

    fn lexical_rows(
        &self,
        query: &str,
        filter: &ScopeFilter,
        limit: u16,
        cancellation: &RecallCancellation,
    ) -> Result<Vec<String>, RecallIndexError> {
        let expression = fts_expression(query);
        if limit == 0 || expression.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT span_id FROM recall_spike_fts
             WHERE recall_spike_fts MATCH ?1{}
             ORDER BY bm25(recall_spike_fts),span_id LIMIT ?2",
            filter.sql
        );
        let mut values = vec![
            rusqlite::types::Value::Text(expression),
            rusqlite::types::Value::Integer(i64::from(limit)),
        ];
        values.extend(filter.values.clone());
        let mut statement = self.connection.prepare(&sql)?;
        let result = statement
            .query_map(params_from_iter(values), |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into);
        map_cancellation(result, cancellation)
    }
}

#[derive(Clone)]
struct ScopeFilter {
    sql: &'static str,
    values: Vec<rusqlite::types::Value>,
}

impl ScopeFilter {
    fn new(scope: RecallScope) -> Result<Self, RecallIndexError> {
        match scope {
            RecallScope::Library => Ok(Self {
                sql: "",
                values: Vec::new(),
            }),
            RecallScope::Podcast { podcast_id } => Ok(Self {
                sql: " AND podcast_id=?3",
                values: vec![rusqlite::types::Value::Text(podcast_key(podcast_id))],
            }),
            RecallScope::Episode { episode_id } => Ok(Self {
                sql: " AND episode_id=?3",
                values: vec![rusqlite::types::Value::Text(episode_key(episode_id))],
            }),
            RecallScope::Unsupported { .. } => Err(RecallIndexError::InvalidInput(
                "unsupported recall scope cannot reach the index",
            )),
        }
    }
}

fn combine_candidates(
    connection: &rusqlite::Connection,
    vector: &[String],
    lexical: &[String],
    maximum_total: u16,
) -> Result<Vec<RecallCandidateObservation>, RecallIndexError> {
    let mut ranks: BTreeMap<&str, (Option<u16>, Option<u16>)> = BTreeMap::new();
    add_ranks(&mut ranks, vector, true)?;
    add_ranks(&mut ranks, lexical, false)?;
    if ranks.len() > usize::from(maximum_total) {
        return Err(RecallIndexError::InvalidInput(
            "recall candidate union exceeds its declared bound",
        ));
    }
    let mut metadata = connection
        .prepare("SELECT generation_id,episode_id FROM recall_spike_meta WHERE span_id=?1")?;
    ranks
        .into_iter()
        .map(|(span_key, (vector_rank, lexical_rank))| {
            let row = metadata
                .query_row(params![span_key], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .optional()?
                .ok_or(RecallIndexError::InvalidInput(
                    "recall candidate metadata is incomplete",
                ))?;
            Ok(RecallCandidateObservation {
                episode_id: parse_episode_key(&row.1).ok_or(RecallIndexError::InvalidInput(
                    "recall episode identity is malformed",
                ))?,
                generation_id: parse_generation_key(&row.0).ok_or(
                    RecallIndexError::InvalidInput("recall generation identity is malformed"),
                )?,
                span_id: parse_span_key(span_key).ok_or(RecallIndexError::InvalidInput(
                    "recall span identity is malformed",
                ))?,
                vector_rank,
                lexical_rank,
            })
        })
        .collect()
}

fn map_cancellation<T>(
    result: Result<T, RecallIndexError>,
    cancellation: &RecallCancellation,
) -> Result<T, RecallIndexError> {
    match result {
        Err(_) if cancellation.is_cancelled() => Err(RecallIndexError::Cancelled),
        value => value,
    }
}

fn add_ranks<'a>(
    ranks: &mut BTreeMap<&'a str, (Option<u16>, Option<u16>)>,
    keys: &'a [String],
    vector: bool,
) -> Result<(), RecallIndexError> {
    for (offset, key) in keys.iter().enumerate() {
        let rank = u16::try_from(offset + 1)
            .map_err(|_| RecallIndexError::InvalidInput("recall rank exceeds UInt16"))?;
        let entry = ranks.entry(key).or_default();
        if vector {
            entry.0 = Some(rank);
        } else {
            entry.1 = Some(rank);
        }
    }
    Ok(())
}

fn fts_expression(query: &str) -> String {
    query
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(|token| format!("\"{token}\""))
        .collect::<Vec<_>>()
        .join(" AND ")
}
