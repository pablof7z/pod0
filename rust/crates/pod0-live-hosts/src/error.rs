use std::{fmt, io};

use crate::{HttpEvidence, ProviderKind};

#[derive(Debug)]
#[non_exhaustive]
pub enum AdapterError {
    Unavailable(UnavailableError),
    Credential(CredentialError),
    Network(NetworkError),
    Provider(ProviderError),
    Protocol(ProtocolError),
    Size(SizeError),
    File(FileError),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnavailableError {
    pub capability: &'static str,
    pub status: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialError {
    pub provider: ProviderKind,
    pub reason: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkErrorKind {
    Timeout,
    Connect,
    Request,
    ResponseBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkError {
    pub kind: NetworkErrorKind,
}

pub struct ProviderError {
    pub provider: ProviderKind,
    pub status: u16,
    pub retry_after: Option<String>,
    pub request_id: Option<String>,
    pub body: Vec<u8>,
    pub body_truncated: bool,
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderError")
            .field("provider", &self.provider)
            .field("status", &self.status)
            .field("retry_after", &self.retry_after)
            .field("request_id", &self.request_id)
            .field("body_bytes", &self.body.len())
            .field("body_truncated", &self.body_truncated)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    pub context: &'static str,
    pub status: Option<u16>,
    pub evidence: Option<Box<HttpEvidence>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeSubject {
    ResponseBody,
    Output,
    Metadata,
    Upload,
    EmbeddingDimensions,
    ResultCount,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SizeError {
    pub subject: SizeSubject,
    pub limit: u64,
    pub observed: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileErrorKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidInput,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileError {
    pub operation: &'static str,
    pub kind: FileErrorKind,
}

impl AdapterError {
    pub(crate) fn response_body() -> Self {
        Self::Network(NetworkError {
            kind: NetworkErrorKind::ResponseBody,
        })
    }

    pub(crate) fn from_reqwest(error: &reqwest::Error) -> Self {
        let kind = if error.is_timeout() {
            NetworkErrorKind::Timeout
        } else if error.is_connect() {
            NetworkErrorKind::Connect
        } else if error.is_body() {
            NetworkErrorKind::ResponseBody
        } else {
            NetworkErrorKind::Request
        };
        Self::Network(NetworkError { kind })
    }

    pub(crate) fn from_io(operation: &'static str, error: &io::Error) -> Self {
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

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(_) => formatter.write_str("capability is unavailable"),
            Self::Credential(_) => formatter.write_str("provider credential is unavailable"),
            Self::Network(_) => formatter.write_str("network request failed"),
            Self::Provider(error) => write!(
                formatter,
                "{} provider returned HTTP {}",
                error.provider.as_str(),
                error.status
            ),
            Self::Protocol(error) => {
                write!(formatter, "invalid {} protocol response", error.context)
            }
            Self::Size(_) => formatter.write_str("bounded data exceeded its limit"),
            Self::File(_) => formatter.write_str("file operation failed"),
            Self::Cancelled => formatter.write_str("request was cancelled"),
        }
    }
}

impl std::error::Error for AdapterError {}
