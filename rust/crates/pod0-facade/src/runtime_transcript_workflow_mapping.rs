use pod0_application::{TranscriptProvider, TranscriptWorkflowOrigin, TranscriptWorkflowRequest};
use pod0_domain::HostRequestId;
use pod0_storage::StoredTranscriptWorkflowRequest;

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
        TranscriptWorkflowOrigin::Playback => "playback",
        TranscriptWorkflowOrigin::Unsupported { .. } => "unsupported",
    }
}

pub(super) fn request_id(
    workflow_id: pod0_domain::TranscriptWorkflowId,
    attempt: u16,
    publisher: bool,
) -> HostRequestId {
    pod0_application::transcript_workflow_request_id(workflow_id, attempt, publisher)
}
