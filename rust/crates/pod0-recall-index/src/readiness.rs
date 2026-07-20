use pod0_domain::{EpisodeId, EvidenceGenerationId};
use rusqlite::{OptionalExtension, params};

use crate::identity::{episode_key, generation_key};
use crate::{RecallIndex, RecallIndexError};

impl RecallIndex {
    pub fn generation_is_ready(
        &self,
        episode_id: EpisodeId,
        generation_id: EvidenceGenerationId,
        expected_span_count: u32,
    ) -> Result<bool, RecallIndexError> {
        let episode = episode_key(episode_id);
        let generation = generation_key(generation_id);
        let stored = self
            .connection
            .query_row(
                "SELECT generation_id,span_count FROM pod0_recall_generations_v1
                 WHERE episode_id=?1",
                [&episode],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)),
            )
            .optional()?;
        if stored != Some((generation.clone(), expected_span_count)) {
            return Ok(false);
        }
        let metadata_count = count(
            self,
            "SELECT COUNT(*) FROM pod0_recall_meta_v1
             WHERE episode_id=?1 AND generation_id=?2",
            &episode,
            &generation,
        )?;
        let vector_count = count(
            self,
            "SELECT COUNT(*) FROM pod0_recall_vec_v1
             WHERE episode_id=?1 AND span_id IN (
               SELECT span_id FROM pod0_recall_meta_v1
               WHERE episode_id=?1 AND generation_id=?2
             )",
            &episode,
            &generation,
        )?;
        let lexical_count = count(
            self,
            "SELECT COUNT(*) FROM pod0_recall_fts_v1
             WHERE episode_id=?1 AND span_id IN (
               SELECT span_id FROM pod0_recall_meta_v1
               WHERE episode_id=?1 AND generation_id=?2
             )",
            &episode,
            &generation,
        )?;
        Ok([metadata_count, vector_count, lexical_count]
            .into_iter()
            .all(|count| count == u64::from(expected_span_count)))
    }
}

fn count(
    index: &RecallIndex,
    sql: &str,
    episode: &str,
    generation: &str,
) -> Result<u64, RecallIndexError> {
    let count: i64 = index
        .connection
        .query_row(sql, params![episode, generation], |row| row.get(0))?;
    u64::try_from(count)
        .map_err(|_| RecallIndexError::InvalidInput("recall readiness count is invalid"))
}
