use std::{path::PathBuf, time::Duration};

use pod0_live_hosts::{
    CancellationToken, ChatMessage, ChatRequest, ChatRole, EmbeddingRequest, HttpLimits, LiveHosts,
    OllamaChatRequest, OpenAiChatRequest, OpenAiEmbeddingRequest, ProviderEndpoint, RerankDocument,
    RerankRequest, SecretString, ToolChoice, TranscriptionRequest, TranscriptionResponseFormat,
};

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this ignored live test"))
}

fn limits() -> HttpLimits {
    HttpLimits {
        maximum_body_bytes: 4 * 1_024 * 1_024,
        maximum_metadata_bytes: 16 * 1_024,
    }
}

fn chat(model: String) -> ChatRequest {
    ChatRequest {
        model,
        messages: vec![ChatMessage::text(
            ChatRole::User,
            "Reply with exactly the word live.",
        )],
        tools: Vec::new(),
        tool_choice: ToolChoice::None,
        temperature: Some(0.0),
        maximum_completion_tokens: Some(16),
        timeout: Duration::from_secs(60),
        limits: limits(),
        maximum_output_bytes: 1_024,
    }
}

#[tokio::test]
#[ignore = "requires POD0_LIVE_OPENAI_CHAT_URL, POD0_LIVE_OPENAI_API_KEY, and POD0_LIVE_OPENAI_MODEL"]
async fn openai_chat_provider_live() {
    let result = LiveHosts::default()
        .openai_chat(
            OpenAiChatRequest {
                endpoint: ProviderEndpoint::bearer(
                    required_env("POD0_LIVE_OPENAI_CHAT_URL"),
                    SecretString::new(required_env("POD0_LIVE_OPENAI_API_KEY")),
                ),
                chat: chat(required_env("POD0_LIVE_OPENAI_MODEL")),
            },
            &CancellationToken::new(),
        )
        .await
        .expect("live OpenAI-compatible chat request");
    assert!(result.content.is_some() || !result.tool_calls.is_empty());
}

#[tokio::test]
#[ignore = "requires POD0_LIVE_OLLAMA_CHAT_URL and POD0_LIVE_OLLAMA_MODEL"]
async fn ollama_chat_provider_live() {
    let result = LiveHosts::default()
        .ollama_chat(
            OllamaChatRequest {
                endpoint: ProviderEndpoint::unauthenticated(required_env(
                    "POD0_LIVE_OLLAMA_CHAT_URL",
                )),
                chat: chat(required_env("POD0_LIVE_OLLAMA_MODEL")),
            },
            &CancellationToken::new(),
        )
        .await
        .expect("live Ollama chat request");
    assert!(result.content.is_some() || !result.tool_calls.is_empty());
}

#[tokio::test]
#[ignore = "requires POD0_LIVE_TRANSCRIPTION_URL, POD0_LIVE_OPENAI_API_KEY, POD0_LIVE_TRANSCRIPTION_MODEL, and POD0_LIVE_AUDIO_PATH"]
async fn transcription_provider_live() {
    let result = LiveHosts::default()
        .transcribe_audio(
            TranscriptionRequest {
                endpoint: ProviderEndpoint::bearer(
                    required_env("POD0_LIVE_TRANSCRIPTION_URL"),
                    SecretString::new(required_env("POD0_LIVE_OPENAI_API_KEY")),
                ),
                model: required_env("POD0_LIVE_TRANSCRIPTION_MODEL"),
                audio_path: PathBuf::from(required_env("POD0_LIVE_AUDIO_PATH")),
                language: None,
                prompt: None,
                temperature: None,
                response_format: TranscriptionResponseFormat::Json,
                timeout: Duration::from_secs(300),
                limits: limits(),
                maximum_upload_bytes: 512 * 1_024 * 1_024,
                maximum_output_bytes: 4 * 1_024 * 1_024,
            },
            &CancellationToken::new(),
        )
        .await
        .expect("live transcription request");
    assert!(!result.text.is_empty());
}

#[tokio::test]
#[ignore = "requires POD0_LIVE_EMBEDDINGS_URL, POD0_LIVE_OPENAI_API_KEY, and POD0_LIVE_EMBEDDINGS_MODEL"]
async fn embeddings_provider_live() {
    let result = LiveHosts::default()
        .openai_embeddings(
            OpenAiEmbeddingRequest {
                endpoint: ProviderEndpoint::bearer(
                    required_env("POD0_LIVE_EMBEDDINGS_URL"),
                    SecretString::new(required_env("POD0_LIVE_OPENAI_API_KEY")),
                ),
                embedding: EmbeddingRequest {
                    model: required_env("POD0_LIVE_EMBEDDINGS_MODEL"),
                    inputs: vec!["portable live embeddings".to_owned()],
                    dimensions: None,
                    timeout: Duration::from_secs(60),
                    limits: limits(),
                    maximum_dimensions: 65_536,
                    maximum_vectors: 1,
                },
            },
            &CancellationToken::new(),
        )
        .await
        .expect("live embeddings request");
    assert_eq!(result.embeddings.len(), 1);
    assert!(!result.embeddings[0].values.is_empty());
}

#[tokio::test]
#[ignore = "requires POD0_LIVE_RERANK_URL, POD0_LIVE_RERANK_API_KEY, and POD0_LIVE_RERANK_MODEL"]
async fn rerank_provider_live() {
    let result = LiveHosts::default()
        .rerank(
            RerankRequest {
                endpoint: ProviderEndpoint::bearer(
                    required_env("POD0_LIVE_RERANK_URL"),
                    SecretString::new(required_env("POD0_LIVE_RERANK_API_KEY")),
                ),
                model: required_env("POD0_LIVE_RERANK_MODEL"),
                query: "portable execution".to_owned(),
                documents: vec![
                    RerankDocument {
                        id: "relevant".to_owned(),
                        text: "portable execution adapters".to_owned(),
                    },
                    RerankDocument {
                        id: "other".to_owned(),
                        text: "gardening notes".to_owned(),
                    },
                ],
                top_n: Some(2),
                timeout: Duration::from_secs(60),
                limits: limits(),
                maximum_results: 2,
            },
            &CancellationToken::new(),
        )
        .await
        .expect("live rerank request");
    assert!(!result.results.is_empty());
}
