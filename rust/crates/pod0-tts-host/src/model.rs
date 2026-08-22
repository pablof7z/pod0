use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{ElevenLabsEndpoint, ProviderSecret};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TtsLimits {
    pub maximum_script_bytes: usize,
    pub maximum_output_bytes: u64,
}

pub struct GenerationRequest<'a> {
    pub endpoint: &'a ElevenLabsEndpoint,
    pub secret: &'a ProviderSecret,
    pub model_id: &'a str,
    pub voice_id: &'a str,
    pub script: &'a str,
    pub output_format: Option<&'a str>,
    pub staged_path: &'a Path,
    pub timeout: Duration,
    pub limits: TtsLimits,
}

impl fmt::Debug for GenerationRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GenerationRequest")
            .field("endpoint", self.endpoint)
            .field("secret", &"[REDACTED]")
            .field("model_id", &self.model_id)
            .field("voice_id", &self.voice_id)
            .field("script_bytes", &self.script.len())
            .field("output_format", &self.output_format)
            .field("staged_path", &self.staged_path)
            .field("timeout", &self.timeout)
            .field("limits", &self.limits)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioMediaType {
    Mpeg,
    Ogg,
    Opus,
    Wav,
    Flac,
    Aac,
    Mp4,
    Pcm,
    Ulaw,
    Alaw,
    Basic,
}

impl AudioMediaType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mpeg => "audio/mpeg",
            Self::Ogg => "audio/ogg",
            Self::Opus => "audio/opus",
            Self::Wav => "audio/wav",
            Self::Flac => "audio/flac",
            Self::Aac => "audio/aac",
            Self::Mp4 => "audio/mp4",
            Self::Pcm => "audio/pcm",
            Self::Ulaw => "audio/ulaw",
            Self::Alaw => "audio/alaw",
            Self::Basic => "audio/basic",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "audio/mpeg" | "audio/mp3" => Some(Self::Mpeg),
            "audio/ogg" => Some(Self::Ogg),
            "audio/opus" => Some(Self::Opus),
            "audio/wav" | "audio/wave" | "audio/x-wav" | "audio/vnd.wave" => Some(Self::Wav),
            "audio/flac" | "audio/x-flac" => Some(Self::Flac),
            "audio/aac" => Some(Self::Aac),
            "audio/mp4" => Some(Self::Mp4),
            "audio/pcm" | "audio/l16" => Some(Self::Pcm),
            "audio/ulaw" | "audio/x-mulaw" => Some(Self::Ulaw),
            "audio/alaw" | "audio/x-alaw" => Some(Self::Alaw),
            "audio/basic" => Some(Self::Basic),
            _ => None,
        }
    }

    pub(crate) fn for_output_format(value: Option<&str>) -> Option<Self> {
        let Some(value) = value else {
            return Some(Self::Mpeg);
        };
        let family = value
            .split_once(['_', '-'])
            .map_or(value, |(family, _)| family);
        match family {
            "mp3" | "mpeg" => Some(Self::Mpeg),
            "ogg" => Some(Self::Ogg),
            "opus" => Some(Self::Opus),
            "wav" | "wave" => Some(Self::Wav),
            "flac" => Some(Self::Flac),
            "aac" => Some(Self::Aac),
            "mp4" => Some(Self::Mp4),
            "pcm" => Some(Self::Pcm),
            "ulaw" => Some(Self::Ulaw),
            "alaw" => Some(Self::Alaw),
            "basic" | "au" => Some(Self::Basic),
            _ => None,
        }
    }

    pub(crate) fn accepts_declaration(self, declared: Self) -> bool {
        self == declared || matches!((self, declared), (Self::Opus, Self::Ogg))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderResponseEvidence {
    pub status: u16,
    pub request_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationEvidence {
    pub staged_path: PathBuf,
    pub media_type: AudioMediaType,
    pub byte_count: u64,
    pub content_digest: [u8; 32],
    pub provider: ProviderResponseEvidence,
}
