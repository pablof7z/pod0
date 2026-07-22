use pod0_application::{
    HostRequest, HostRequestEnvelope, MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
    TranscriptCapabilityContext, TranscriptCapabilityRequest, TranscriptProvider,
    TranscriptWorkflowOrigin, TranscriptWorkflowRequest,
};
use pod0_domain::{HostRequestId, UnixTimestampMilliseconds};
use pod0_storage::{StoredTranscriptWorkflowRequest, TranscriptWorkflowRecord};
use sha2::{Digest as _, Sha256};

pub(super) fn stored_request(
    request: TranscriptWorkflowRequest,
) -> StoredTranscriptWorkflowRequest {
    StoredTranscriptWorkflowRequest {
        workflow_id: request.workflow_id,
        source_revision: request.source_revision,
        origin: origin_wire(request.origin).to_owned(),
        provider: provider_wire(request.provider).to_owned(),
        model: request.model,
        remote_audio_url: request.remote_audio_url,
        local_audio_url: request.local_audio_url,
        publisher_transcript_url: request.publisher_transcript_url,
        publisher_mime_hint: request.publisher_mime_hint,
        publisher_first: request.publisher_first,
        provider_fallback_enabled: request.provider_fallback_enabled,
    }
}

pub(super) fn host_request(
    record: &TranscriptWorkflowRecord,
    podcast_id: pod0_domain::PodcastId,
) -> Option<HostRequestEnvelope> {
    let context = TranscriptCapabilityContext {
        episode_id: record.episode_id,
        podcast_id,
        source_revision: record.request.source_revision.clone(),
    };
    let capability = match record.stage {
        pod0_storage::StoredTranscriptWorkflowStage::PublisherRequested => {
            TranscriptCapabilityRequest::FetchPublisher {
                context,
                source_url: record.request.publisher_transcript_url.clone()?,
                mime_hint: record.request.publisher_mime_hint.clone(),
                maximum_response_bytes: MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
            }
        }
        pod0_storage::StoredTranscriptWorkflowStage::SubmissionAuthorized => {
            let attempt_id = record.attempt_id?;
            match provider(&record.request.provider) {
                TranscriptProvider::AppleSpeech => TranscriptCapabilityRequest::TranscribeLocal {
                    context,
                    attempt_id,
                    audio_url: record.request.local_audio_url.clone()?,
                    locale: None,
                },
                provider => TranscriptCapabilityRequest::SubmitProvider {
                    context,
                    attempt_id,
                    submission_fence_id: record.submission_fence_id?,
                    provider,
                    model: record.request.model.clone(),
                    audio_url: record.request.remote_audio_url.clone(),
                    maximum_response_bytes: MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
                },
            }
        }
        pod0_storage::StoredTranscriptWorkflowStage::ProviderAccepted => {
            TranscriptCapabilityRequest::RecoverProvider {
                context,
                attempt_id: record.attempt_id?,
                submission_fence_id: record.submission_fence_id?,
                provider: provider(&record.request.provider),
                model: record.request.model.clone(),
                external_operation_id: record.external_operation_id.clone()?,
                provider_status: record.provider_status.clone(),
                maximum_response_bytes: MAX_TRANSCRIPT_CAPABILITY_RESPONSE_BYTES,
            }
        }
        _ => return None,
    };
    Some(HostRequestEnvelope {
        request_id: record.request_id?,
        command_id: record.command_id,
        cancellation_id: record.cancellation_id,
        issued_revision: record.issued_revision,
        deadline_at: (record.stage
            != pod0_storage::StoredTranscriptWorkflowStage::ProviderAccepted)
            .then_some(record.deadline_at_ms)
            .flatten()
            .map(UnixTimestampMilliseconds::new),
        request: HostRequest::ExecuteTranscriptCapability { capability },
    })
}

pub(super) fn provider(value: &str) -> TranscriptProvider {
    match value {
        "assembly-ai" => TranscriptProvider::AssemblyAi,
        "elevenlabs-scribe" => TranscriptProvider::ElevenLabsScribe,
        "openrouter-whisper" => TranscriptProvider::OpenRouterWhisper,
        "apple-speech" => TranscriptProvider::AppleSpeech,
        _ => TranscriptProvider::Unsupported { wire_code: 1 },
    }
}

pub(super) const fn provider_wire(value: TranscriptProvider) -> &'static str {
    match value {
        TranscriptProvider::AssemblyAi => "assembly-ai",
        TranscriptProvider::ElevenLabsScribe => "elevenlabs-scribe",
        TranscriptProvider::OpenRouterWhisper => "openrouter-whisper",
        TranscriptProvider::AppleSpeech => "apple-speech",
        TranscriptProvider::Unsupported { .. } => "unsupported",
    }
}

const fn origin_wire(value: TranscriptWorkflowOrigin) -> &'static str {
    match value {
        TranscriptWorkflowOrigin::User => "user",
        TranscriptWorkflowOrigin::Automatic => "automatic",
        TranscriptWorkflowOrigin::Unsupported { .. } => "unsupported",
    }
}

pub(super) fn request_id(
    workflow_id: pod0_domain::TranscriptWorkflowId,
    attempt: u16,
    publisher: bool,
) -> HostRequestId {
    let mut hash = Sha256::new();
    hash.update(b"pod0-transcript-host-request-v1\0");
    hash.update(workflow_id.into_bytes());
    hash.update(attempt.to_be_bytes());
    hash.update([u8::from(publisher)]);
    let digest = hash.finalize();
    HostRequestId::from_bytes(digest[..16].try_into().expect("digest prefix"))
}
