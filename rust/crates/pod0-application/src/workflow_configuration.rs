use pod0_domain::{ContentDigest, EpisodeId, StateRevision, UnixTimestampMilliseconds};
use sha2::{Digest, Sha256};

use crate::TranscriptProvider;

pub const WORKFLOW_CONFIGURATION_SCHEMA_VERSION: u32 = 1;
pub const MAX_WORKFLOW_MODEL_REFERENCE_BYTES: usize = 256;
pub const MAX_WORKFLOW_LOCAL_AUDIO_CAPABILITIES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct WorkflowConfigurationInput {
    pub transcript_provider: TranscriptProvider,
    pub eleven_labs_model: String,
    pub assembly_ai_model: String,
    pub open_router_model: String,
    pub auto_publisher_transcripts: bool,
    pub auto_provider_transcripts: bool,
    pub chapter_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct WorkflowConfiguration {
    pub schema_version: u32,
    pub revision: StateRevision,
    pub origin: WorkflowConfigurationOrigin,
    pub value: WorkflowConfigurationInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum WorkflowConfigurationOrigin {
    LegacySwiftImport,
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum WorkflowConfigurationFailure {
    UnsupportedProvider,
    InvalidModelReference,
    UnsupportedSchema,
    InvalidCapabilitySnapshot,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record,
)]
pub struct TranscriptCredentialCapabilities {
    pub eleven_labs: bool,
    pub assembly_ai: bool,
    pub open_router: bool,
    pub apple_speech: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct LocalAudioCapability {
    pub episode_id: EpisodeId,
    pub local_audio_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct WorkflowCapabilitySnapshot {
    pub snapshot_id: ContentDigest,
    pub observed_at: UnixTimestampMilliseconds,
    pub credentials: TranscriptCredentialCapabilities,
    pub local_audio: Vec<LocalAudioCapability>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct WorkflowCapabilitySnapshotInput {
    pub credentials: TranscriptCredentialCapabilities,
    pub local_audio: Vec<LocalAudioCapability>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum WorkflowOpportunityReason {
    Launch,
    Foreground,
    LibraryChanged,
    ConfigurationChanged,
    CredentialChanged,
    LocalAudioChanged,
    ScheduledWake,
    Unsupported { wire_code: u32 },
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record,
)]
pub struct WorkflowOpportunity {
    pub reason: WorkflowOpportunityReason,
    pub observed_at: UnixTimestampMilliseconds,
    pub capability_snapshot_id: ContentDigest,
}

impl WorkflowConfigurationInput {
    pub fn validate(&self) -> Result<(), WorkflowConfigurationFailure> {
        if matches!(
            self.transcript_provider,
            TranscriptProvider::Unsupported { .. }
        ) {
            return Err(WorkflowConfigurationFailure::UnsupportedProvider);
        }
        for model in [
            &self.eleven_labs_model,
            &self.assembly_ai_model,
            &self.open_router_model,
            &self.chapter_model,
        ] {
            if !valid_model(model) {
                return Err(WorkflowConfigurationFailure::InvalidModelReference);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn selected_transcript_model(&self) -> Option<&str> {
        match self.transcript_provider {
            TranscriptProvider::ElevenLabsScribe => Some(&self.eleven_labs_model),
            TranscriptProvider::AssemblyAi => Some(&self.assembly_ai_model),
            TranscriptProvider::OpenRouterWhisper => Some(&self.open_router_model),
            TranscriptProvider::AppleSpeech => Some("apple-native-v1"),
            TranscriptProvider::Unsupported { .. } => None,
        }
    }
}

impl TranscriptCredentialCapabilities {
    #[must_use]
    pub const fn available(self, provider: TranscriptProvider) -> bool {
        match provider {
            TranscriptProvider::ElevenLabsScribe => self.eleven_labs,
            TranscriptProvider::AssemblyAi => self.assembly_ai,
            TranscriptProvider::OpenRouterWhisper => self.open_router,
            TranscriptProvider::AppleSpeech => self.apple_speech,
            TranscriptProvider::Unsupported { .. } => false,
        }
    }
}

impl WorkflowCapabilitySnapshot {
    pub fn from_input(
        mut input: WorkflowCapabilitySnapshotInput,
        observed_at: UnixTimestampMilliseconds,
    ) -> Result<Self, WorkflowConfigurationFailure> {
        input.local_audio.sort_by_key(|value| value.episode_id);
        input.local_audio.dedup_by_key(|value| value.episode_id);
        if observed_at.value < 0
            || input.local_audio.len() > MAX_WORKFLOW_LOCAL_AUDIO_CAPABILITIES
            || input
                .local_audio
                .iter()
                .any(|value| !valid_local_audio_url(&value.local_audio_url))
        {
            return Err(WorkflowConfigurationFailure::InvalidCapabilitySnapshot);
        }
        let mut hash = Sha256::new();
        hash.update(b"pod0:workflow-capability-snapshot:v1");
        hash.update([
            u8::from(input.credentials.eleven_labs),
            u8::from(input.credentials.assembly_ai),
            u8::from(input.credentials.open_router),
            u8::from(input.credentials.apple_speech),
        ]);
        for capability in &input.local_audio {
            hash.update(capability.episode_id.into_bytes());
            hash.update(capability.local_audio_url.as_bytes());
        }
        Ok(Self {
            snapshot_id: ContentDigest::from_bytes(hash.finalize().into()),
            observed_at,
            credentials: input.credentials,
            local_audio: input.local_audio,
        })
    }
}

fn valid_model(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= MAX_WORKFLOW_MODEL_REFERENCE_BYTES
        && !value.chars().any(char::is_control)
}

fn valid_local_audio_url(value: &str) -> bool {
    value.len() <= 2_048
        && url::Url::parse(value).is_ok_and(|url| {
            url.scheme() == "file"
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
        })
}
