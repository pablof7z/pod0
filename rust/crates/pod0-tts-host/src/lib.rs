mod audio;
mod cancellation;
mod client;
mod endpoint;
mod error;
mod file;
mod model;
mod opus;
mod response;
mod secret;
mod validation;

pub use cancellation::CancellationToken;
pub use client::{ClientConfig, TtsClient};
pub use endpoint::ElevenLabsEndpoint;
pub use error::{
    CredentialError, CredentialErrorKind, FileError, FileErrorKind, FileOperation,
    InvalidRequestError, InvalidRequestField, InvalidRequestReason, LimitError, LimitSubject,
    NetworkError, NetworkErrorKind, ProtocolError, ProtocolErrorKind, ProviderError, TtsError,
};
pub use model::{
    AudioMediaType, GenerationEvidence, GenerationRequest, ProviderResponseEvidence, TtsLimits,
};
pub use secret::ProviderSecret;
