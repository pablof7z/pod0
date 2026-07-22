use pod0_domain::{
    ContentDigest, EpisodeId, SpeakerId, TranscriptAttemptId, TranscriptSubmissionFenceId,
    TranscriptVersionId, TranscriptWorkflowId, UnixTimestampMilliseconds,
};
use sha2::{Digest as _, Sha256};

use crate::{
    TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS, TRANSCRIPT_RETRY_BASE_MILLISECONDS,
    TRANSCRIPT_RETRY_MAX_MILLISECONDS, TranscriptProvider,
};

const MAX_EMBEDDING_SPACE_ID_BYTES: usize = 256;

#[must_use]
pub fn transcript_workflow_id(
    episode_id: EpisodeId,
    source_revision: &str,
    provider: TranscriptProvider,
    model: &str,
) -> TranscriptWorkflowId {
    let mut hash = FramedHash::new(b"pod0-transcript-workflow-v1");
    hash.bytes(&episode_id.into_bytes());
    hash.string(source_revision);
    hash.string(provider_wire_name(provider));
    hash.string(model);
    TranscriptWorkflowId::from_bytes(hash.first_16())
}

/// Matches the existing iOS audio-version contract so selected transcripts
/// remain current across the source-of-truth cutover.
#[must_use]
pub fn transcript_source_revision(
    enclosure_url: &str,
    enclosure_mime_type: Option<&str>,
    duration_milliseconds: Option<u64>,
) -> Option<String> {
    crate::normalize_media_url(enclosure_url)?;
    let seconds = duration_milliseconds.map_or(0.0, |value| value as f64 / 1_000.0);
    let duration = if seconds.fract() == 0.0 {
        format!("{seconds:.1}")
    } else {
        seconds.to_string()
    };
    let parts = [enclosure_url, enclosure_mime_type.unwrap_or(""), &duration];
    let mut hash = Sha256::new();
    hash.update(parts.join("\u{1f}").as_bytes());
    Some(format!("{:x}", hash.finalize()))
}

#[must_use]
pub fn transcript_attempt_id(
    workflow_id: TranscriptWorkflowId,
    attempt: u16,
) -> Option<TranscriptAttemptId> {
    if attempt == 0 {
        return None;
    }
    let mut hash = FramedHash::new(b"pod0-transcript-attempt-v1");
    hash.bytes(&workflow_id.into_bytes());
    hash.u64(u64::from(attempt));
    Some(TranscriptAttemptId::from_bytes(hash.first_16()))
}

#[must_use]
pub fn transcript_submission_fence_id(
    attempt_id: TranscriptAttemptId,
) -> TranscriptSubmissionFenceId {
    let mut hash = FramedHash::new(b"pod0-transcript-submission-fence-v1");
    hash.bytes(&attempt_id.into_bytes());
    TranscriptSubmissionFenceId::from_bytes(hash.first_16())
}

#[must_use]
pub fn transcript_speaker_id(
    episode_id: EpisodeId,
    source_revision: &str,
    label: &str,
) -> Option<SpeakerId> {
    if source_revision.is_empty()
        || source_revision.trim() != source_revision
        || source_revision.len() > pod0_domain::MAX_SOURCE_REVISION_BYTES
        || label.trim().is_empty()
        || label.len() > pod0_domain::MAX_TRANSCRIPT_SPEAKER_LABEL_BYTES
    {
        return None;
    }
    let mut hash = FramedHash::new(b"pod0-transcript-speaker-v1");
    hash.bytes(&episode_id.into_bytes());
    hash.string(source_revision);
    hash.string(label);
    Some(SpeakerId::from_bytes(hash.first_16()))
}

#[must_use]
pub fn transcript_evidence_input_version(
    transcript_version_id: TranscriptVersionId,
    content_digest: ContentDigest,
    embedding_space_id: &str,
) -> Option<String> {
    if embedding_space_id.is_empty() || embedding_space_id.len() > MAX_EMBEDDING_SPACE_ID_BYTES {
        return None;
    }
    let parts = [
        value_hex(&transcript_version_id.into_bytes()),
        value_hex(&content_digest.into_bytes()),
        embedding_space_id.to_owned(),
        "rust-evidence-v1".to_owned(),
        "core-recall-index-v1".to_owned(),
    ];
    let mut hash = Sha256::new();
    hash.update(parts.join("\u{1f}").as_bytes());
    Some(format!("{:x}", hash.finalize()))
}

#[must_use]
pub fn transcript_retry_delay_milliseconds(
    attempt: u16,
    provider_retry_after_milliseconds: Option<i64>,
) -> i64 {
    let exponent = u32::from(attempt.saturating_sub(1).min(10));
    let policy = TRANSCRIPT_RETRY_BASE_MILLISECONDS
        .saturating_mul(2_i64.saturating_pow(exponent))
        .min(TRANSCRIPT_RETRY_MAX_MILLISECONDS);
    provider_retry_after_milliseconds
        .unwrap_or(0)
        .clamp(0, TRANSCRIPT_RETRY_MAX_MILLISECONDS)
        .max(policy)
}

#[must_use]
pub fn transcript_retry_not_before(
    observed_at: UnixTimestampMilliseconds,
    attempt: u16,
    provider_retry_after_milliseconds: Option<i64>,
) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(observed_at.value().saturating_add(
        transcript_retry_delay_milliseconds(attempt, provider_retry_after_milliseconds),
    ))
}

#[must_use]
pub const fn transcript_host_request_deadline(
    issued_at: UnixTimestampMilliseconds,
) -> UnixTimestampMilliseconds {
    UnixTimestampMilliseconds::new(
        issued_at
            .value()
            .saturating_add(TRANSCRIPT_HOST_REQUEST_DEADLINE_MILLISECONDS),
    )
}

const fn provider_wire_name(provider: TranscriptProvider) -> &'static str {
    match provider {
        TranscriptProvider::AssemblyAi => "assembly-ai",
        TranscriptProvider::ElevenLabsScribe => "elevenlabs-scribe",
        TranscriptProvider::OpenRouterWhisper => "openrouter-whisper",
        TranscriptProvider::AppleSpeech => "apple-speech",
        TranscriptProvider::Unsupported { .. } => "unsupported",
    }
}

fn value_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct FramedHash(Sha256);

impl FramedHash {
    fn new(domain: &[u8]) -> Self {
        let mut value = Self(Sha256::new());
        value.bytes(domain);
        value
    }

    fn bytes(&mut self, value: &[u8]) {
        self.0.update((value.len() as u64).to_be_bytes());
        self.0.update(value);
    }

    fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn first_16(self) -> [u8; 16] {
        self.0.finalize()[..16].try_into().expect("digest prefix")
    }
}
