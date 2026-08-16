use pod0_application::{
    DurableTranscriptEffectRequest, MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
    TranscriptCapabilityContext, TranscriptCapabilityRequest, TranscriptProvider,
};
use pod0_domain::{PodcastId, UnixTimestampMilliseconds};
use rusqlite::OptionalExtension;

use crate::{StorageError, StoredTranscriptWorkflowStage, TranscriptWorkflowRecord};

pub(crate) fn exact_transcript_request(
    transaction: &rusqlite::Transaction<'_>,
    record: &TranscriptWorkflowRecord,
    include_deadline: bool,
) -> Result<DurableTranscriptEffectRequest, StorageError> {
    let podcast: Option<Vec<u8>> = transaction
        .query_row(
            "SELECT podcast_id FROM pod0_episodes WHERE episode_id=?1",
            [record.episode_id.into_bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| StorageError::sqlite("read transcript podcast identity", error))?;
    let podcast_id = PodcastId::from_bytes(
        podcast
            .ok_or(StorageError::TranscriptWorkflowNotFound)?
            .try_into()
            .map_err(|_| StorageError::InvalidActivity)?,
    );
    let context = TranscriptCapabilityContext {
        episode_id: record.episode_id,
        podcast_id,
        source_revision: record.request.source_revision.clone(),
    };
    let capability = match record.stage {
        StoredTranscriptWorkflowStage::PublisherRequested => {
            TranscriptCapabilityRequest::FetchPublisher {
                context,
                source_url: record
                    .request
                    .publisher_transcript_url
                    .clone()
                    .ok_or(StorageError::TranscriptWorkflowConflict)?,
                mime_hint: record.request.publisher_mime_hint.clone(),
                maximum_response_bytes: MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
            }
        }
        StoredTranscriptWorkflowStage::Requested
        | StoredTranscriptWorkflowStage::RetryScheduled
        | StoredTranscriptWorkflowStage::SubmissionAuthorized => {
            match provider(&record.request.provider) {
                TranscriptProvider::AppleSpeech => TranscriptCapabilityRequest::TranscribeLocal {
                    context,
                    attempt_id: record
                        .attempt_id
                        .ok_or(StorageError::StaleTranscriptAttempt)?,
                    audio_url: record
                        .request
                        .local_audio_url
                        .clone()
                        .ok_or(StorageError::TranscriptWorkflowConflict)?,
                    locale: None,
                },
                provider => TranscriptCapabilityRequest::SubmitProvider {
                    context,
                    attempt_id: record
                        .attempt_id
                        .ok_or(StorageError::StaleTranscriptAttempt)?,
                    submission_fence_id: record
                        .submission_fence_id
                        .ok_or(StorageError::StaleTranscriptAttempt)?,
                    provider,
                    model: record.request.model.clone(),
                    audio_url: record.request.remote_audio_url.clone(),
                    maximum_response_bytes: MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
                },
            }
        }
        StoredTranscriptWorkflowStage::ProviderAccepted => {
            TranscriptCapabilityRequest::RecoverProvider {
                context,
                attempt_id: record
                    .attempt_id
                    .ok_or(StorageError::StaleTranscriptAttempt)?,
                submission_fence_id: record
                    .submission_fence_id
                    .ok_or(StorageError::StaleTranscriptAttempt)?,
                provider: provider(&record.request.provider),
                model: record.request.model.clone(),
                external_operation_id: record
                    .external_operation_id
                    .clone()
                    .ok_or(StorageError::TranscriptWorkflowConflict)?,
                provider_status: record.provider_status.clone(),
                maximum_response_bytes: MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
            }
        }
        _ => return Err(StorageError::TranscriptWorkflowConflict),
    };
    Ok(DurableTranscriptEffectRequest {
        request_id: record
            .request_id
            .ok_or(StorageError::StaleTranscriptAttempt)?,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: include_deadline
            .then_some(record.deadline_at_ms)
            .flatten()
            .map(UnixTimestampMilliseconds::new),
        capability,
    })
}

fn provider(value: &str) -> TranscriptProvider {
    match value {
        "assembly-ai" => TranscriptProvider::AssemblyAi,
        "elevenlabs-scribe" => TranscriptProvider::ElevenLabsScribe,
        "openrouter-whisper" => TranscriptProvider::OpenRouterWhisper,
        "apple-speech" => TranscriptProvider::AppleSpeech,
        _ => TranscriptProvider::Unsupported { wire_code: 1 },
    }
}
