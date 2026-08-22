use std::time::Duration;

use serde_json::{Value, json};

use crate::{
    AdapterError, CancellationToken, HttpEvidence, HttpLimits, LiveHosts, ProviderEndpoint,
    ProviderKind, TokenUsage,
    bounds::bounded_body,
    embedding_response::{parse_ollama, parse_openai},
    provider::invalid_provider_response,
};

#[derive(Clone, Debug)]
pub struct EmbeddingRequest {
    pub model: String,
    pub inputs: Vec<String>,
    pub dimensions: Option<u32>,
    pub timeout: Duration,
    pub limits: HttpLimits,
    pub maximum_dimensions: u32,
    pub maximum_vectors: u32,
}

#[derive(Debug)]
pub struct OpenAiEmbeddingRequest {
    pub endpoint: ProviderEndpoint,
    pub embedding: EmbeddingRequest,
}

#[derive(Debug)]
pub struct OllamaEmbeddingRequest {
    pub endpoint: ProviderEndpoint,
    pub embedding: EmbeddingRequest,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Embedding {
    pub index: u32,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingResponse {
    pub embeddings: Vec<Embedding>,
    pub usage: TokenUsage,
    pub model: Option<String>,
    pub evidence: HttpEvidence,
}

impl LiveHosts {
    pub async fn openai_embeddings(
        &self,
        request: OpenAiEmbeddingRequest,
        cancellation: &CancellationToken,
    ) -> Result<EmbeddingResponse, AdapterError> {
        cancellation.check()?;
        validate_request(&request.embedding)?;
        let body = openai_body(&request.embedding);
        self.run(request.embedding.timeout, cancellation, async {
            let response = self
                .provider_request(&request.endpoint, ProviderKind::Embeddings)?
                .json(&body)
                .send()
                .await
                .map_err(|error| AdapterError::from_reqwest(&error))?;
            let (response, evidence) = self
                .provider_response(response, ProviderKind::Embeddings, request.embedding.limits)
                .await?;
            let bytes = bounded_body(response, request.embedding.limits.maximum_body_bytes).await?;
            parse_openai(
                &bytes,
                evidence,
                request.embedding.inputs.len(),
                request.embedding.dimensions,
                request.embedding.maximum_dimensions,
                request.embedding.maximum_vectors,
            )
        })
        .await
    }

    pub async fn ollama_embeddings(
        &self,
        request: OllamaEmbeddingRequest,
        cancellation: &CancellationToken,
    ) -> Result<EmbeddingResponse, AdapterError> {
        cancellation.check()?;
        validate_request(&request.embedding)?;
        let body = ollama_body(&request.embedding);
        self.run(request.embedding.timeout, cancellation, async {
            let response = self
                .provider_request(&request.endpoint, ProviderKind::Ollama)?
                .json(&body)
                .send()
                .await
                .map_err(|error| AdapterError::from_reqwest(&error))?;
            let (response, evidence) = self
                .provider_response(response, ProviderKind::Ollama, request.embedding.limits)
                .await?;
            let bytes = bounded_body(response, request.embedding.limits.maximum_body_bytes).await?;
            parse_ollama(
                &bytes,
                evidence,
                request.embedding.inputs.len(),
                request.embedding.dimensions,
                request.embedding.maximum_dimensions,
                request.embedding.maximum_vectors,
            )
        })
        .await
    }
}

fn openai_body(request: &EmbeddingRequest) -> Value {
    let mut body = json!({
        "model": request.model,
        "input": request.inputs,
        "encoding_format": "float"
    });
    if let Some(dimensions) = request.dimensions {
        body["dimensions"] = json!(dimensions);
    }
    body
}

fn ollama_body(request: &EmbeddingRequest) -> Value {
    let mut body = json!({
        "model": request.model,
        "input": request.inputs
    });
    if let Some(dimensions) = request.dimensions {
        body["dimensions"] = json!(dimensions);
    }
    body
}

fn validate_request(request: &EmbeddingRequest) -> Result<(), AdapterError> {
    request.limits.validate()?;
    let count = u32::try_from(request.inputs.len()).unwrap_or(u32::MAX);
    if request.model.trim().is_empty()
        || request.inputs.is_empty()
        || request.inputs.iter().any(|input| input.is_empty())
        || request.maximum_dimensions == 0
        || request.maximum_vectors == 0
        || count > request.maximum_vectors
        || request
            .dimensions
            .is_some_and(|dimensions| dimensions == 0 || dimensions > request.maximum_dimensions)
    {
        return Err(invalid_provider_response("embedding request"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "embeddings_tests.rs"]
mod tests;
