use pod0_domain::{ChapterModelSubmissionFenceId, ContentDigest, EpisodeId, HostRequestId};
use rusqlite::{Connection, OptionalExtension, Row};

use super::inputs::ModelChapterCompletionRecord;
use crate::{LibraryStore, StorageError};

impl LibraryStore {
    pub fn model_chapter_completion(
        &self,
        request_id: HostRequestId,
    ) -> Result<Option<ModelChapterCompletionRecord>, StorageError> {
        self.read(|connection| read_completion(connection, request_id))
    }
}

pub(crate) fn read_completion(
    connection: &Connection,
    request_id: HostRequestId,
) -> Result<Option<ModelChapterCompletionRecord>, StorageError> {
    connection
        .query_row(
            "SELECT request_id,episode_id,generation,submission_fence_id,completion,\
             completion_digest,provider,model,prompt_tokens,completion_tokens,cached_tokens,\
             reasoning_tokens,cost_microusd,provider_operation_id,provider_status,\
             generated_at_ms,observed_at_ms FROM pod0_model_chapter_completions \
             WHERE request_id=?1",
            [request_id.into_bytes().as_slice()],
            completion_row,
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read model chapter completion", error))
}

fn completion_row(row: &Row<'_>) -> rusqlite::Result<ModelChapterCompletionRecord> {
    Ok(ModelChapterCompletionRecord {
        request_id: HostRequestId::from_bytes(bytes16(row.get(0)?)?),
        episode_id: EpisodeId::from_bytes(bytes16(row.get(1)?)?),
        generation: unsigned(row.get(2)?)?,
        submission_fence_id: ChapterModelSubmissionFenceId::from_bytes(bytes16(row.get(3)?)?),
        completion: row.get(4)?,
        completion_digest: digest(row.get(5)?)?,
        provider: row.get(6)?,
        model: row.get(7)?,
        prompt_tokens: optional_unsigned(row.get(8)?)?,
        completion_tokens: optional_unsigned(row.get(9)?)?,
        cached_tokens: optional_unsigned(row.get(10)?)?,
        reasoning_tokens: optional_unsigned(row.get(11)?)?,
        cost_microusd: optional_unsigned(row.get(12)?)?,
        provider_operation_id: row.get(13)?,
        provider_status: row.get(14)?,
        generated_at_ms: row.get(15)?,
        observed_at_ms: row.get(16)?,
    })
}

fn bytes16(value: Vec<u8>) -> rusqlite::Result<[u8; 16]> {
    value.try_into().map_err(|_| rusqlite::Error::InvalidQuery)
}

fn digest(value: Vec<u8>) -> rusqlite::Result<ContentDigest> {
    value
        .try_into()
        .map(ContentDigest::from_bytes)
        .map_err(|_| rusqlite::Error::InvalidQuery)
}

fn unsigned(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| rusqlite::Error::InvalidQuery)
}

fn optional_unsigned(value: Option<i64>) -> rusqlite::Result<Option<u64>> {
    value.map(unsigned).transpose()
}
