use std::{fmt, io};

#[derive(Debug)]
#[non_exhaustive]
pub enum TtsError {
    InvalidRequest(InvalidRequestError),
    Credential(CredentialError),
    Network(NetworkError),
    Provider(ProviderError),
    Protocol(ProtocolError),
    Limit(LimitError),
    File(FileError),
    Timeout,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidRequestField {
    Endpoint,
    ModelId,
    VoiceId,
    Script,
    OutputFormat,
    StagedPath,
    Timeout,
    Limits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidRequestReason {
    Empty,
    Invalid,
    Zero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidRequestError {
    pub field: InvalidRequestField,
    pub reason: InvalidRequestReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialErrorKind {
    Missing,
    InvalidHeaderValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CredentialError {
    pub kind: CredentialErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkErrorKind {
    BuildClient,
    Connect,
    Request,
    ResponseBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkError {
    pub kind: NetworkErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderError {
    pub status: u16,
    pub response_body_bytes: u64,
    pub body_truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolErrorKind {
    MissingContentType,
    InvalidContentType,
    UnsupportedContentType,
    MismatchedContentType,
    InvalidMetadata,
    EmptyAudio,
    InvalidAudioPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    pub kind: ProtocolErrorKind,
    pub status: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitSubject {
    Script,
    Output,
    ModelId,
    VoiceId,
    OutputFormat,
    Metadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitError {
    pub subject: LimitSubject,
    pub limit: u64,
    pub observed: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOperation {
    InspectOutput,
    CreateTemporary,
    ReadTemporary,
    WriteTemporary,
    FlushTemporary,
    SyncTemporary,
    FinalizeOutput,
    SyncDirectory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileErrorKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidInput,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileError {
    pub operation: FileOperation,
    pub kind: FileErrorKind,
}

impl TtsError {
    pub(crate) fn from_reqwest(error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            return Self::Timeout;
        }
        let kind = if error.is_connect() {
            NetworkErrorKind::Connect
        } else if error.is_body() || error.is_decode() {
            NetworkErrorKind::ResponseBody
        } else {
            NetworkErrorKind::Request
        };
        Self::Network(NetworkError { kind })
    }

    pub(crate) fn from_io(operation: FileOperation, error: &io::Error) -> Self {
        let kind = match error.kind() {
            io::ErrorKind::NotFound => FileErrorKind::NotFound,
            io::ErrorKind::PermissionDenied => FileErrorKind::PermissionDenied,
            io::ErrorKind::AlreadyExists => FileErrorKind::AlreadyExists,
            io::ErrorKind::InvalidInput | io::ErrorKind::InvalidFilename => {
                FileErrorKind::InvalidInput
            }
            _ => FileErrorKind::Other,
        };
        Self::File(FileError { operation, kind })
    }
}

impl fmt::Display for TtsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(_) => formatter.write_str("invalid TTS generation request"),
            Self::Credential(_) => formatter.write_str("invalid provider credential"),
            Self::Network(_) => formatter.write_str("TTS network request failed"),
            Self::Provider(error) => {
                write!(formatter, "TTS provider returned HTTP {}", error.status)
            }
            Self::Protocol(_) => formatter.write_str("invalid TTS provider response"),
            Self::Limit(_) => formatter.write_str("TTS data exceeded its configured bound"),
            Self::File(_) => formatter.write_str("TTS output file operation failed"),
            Self::Timeout => formatter.write_str("TTS generation timed out"),
            Self::Cancelled => formatter.write_str("TTS generation was cancelled"),
        }
    }
}

impl std::error::Error for TtsError {}
