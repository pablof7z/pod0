//! Bounded, cancellable live HTTP, download, and model-provider primitives.
//!
//! This crate deliberately contains no Pod0 retry, scheduling, or workflow policy.

#![forbid(unsafe_code)]

mod bounds;
mod cancellation;
mod chat;
mod chat_request;
mod client;
mod download;
mod embedding_response;
mod embeddings;
mod error;
mod http;
mod ollama_chat;
mod openai_chat;
mod provider;
mod rerank;
mod secret;
mod transcription;
mod transcription_upload;
mod url_debug;

pub use cancellation::CancellationToken;
pub use chat::{
    AssistantToolCall, ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatTool,
    OllamaChatRequest, OpenAiChatRequest, TokenUsage, ToolCall, ToolChoice,
};
pub use client::{ClientConfig, LiveHosts};
pub use download::{DownloadRequest, DownloadResponse};
pub use embeddings::{
    Embedding, EmbeddingRequest, EmbeddingResponse, OllamaEmbeddingRequest, OpenAiEmbeddingRequest,
};
pub use error::{
    AdapterError, CredentialError, FileError, FileErrorKind, NetworkError, NetworkErrorKind,
    ProtocolError, ProviderError, SizeError, SizeSubject, UnavailableError,
};
pub use http::{
    HttpEvidence, HttpGetRequest, HttpGetResponse, HttpLimits, RedirectEvidence, RequestOptions,
};
pub use provider::{CredentialRequirement, ProviderEndpoint, ProviderKind};
pub use rerank::{RerankDocument, RerankRequest, RerankResponse, RerankResult};
pub use secret::SecretString;
pub use transcription::{TranscriptionRequest, TranscriptionResponse, TranscriptionResponseFormat};
