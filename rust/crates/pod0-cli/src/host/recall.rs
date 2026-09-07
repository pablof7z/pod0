mod support;

use support::*;

use std::time::Duration;

use pod0_facade::{
    EvidenceGenerationId, HostFailureCode, HostObservation, RecallEmbeddingInput,
    RecallEmbeddingProvider, RecallEmbeddingVector, RecallRerankDocument, RecallRerankObservation,
    RecallRerankProvider, RecallSpanEmbeddingObservation,
};
use pod0_live_hosts::{
    AdapterError, CancellationToken, CredentialRequirement, EmbeddingRequest, HttpLimits,
    OllamaEmbeddingRequest, OpenAiEmbeddingRequest, ProviderEndpoint, RerankDocument,
    RerankRequest, SecretString,
};

use super::HostExecutor;
use crate::host::failed;

/// Quantization scale: provider floats are stored as signed millionths.
const MILLIONTHS: f32 = 1_000_000.0;

const EMBEDDING_TIMEOUT: Duration = Duration::from_secs(60);
const EMBEDDING_MAXIMUM_BODY_BYTES: u64 = 2 * 1024 * 1024;
const EMBEDDING_MAXIMUM_METADATA_BYTES: u64 = 64 * 1024;
const RERANK_MAXIMUM_BODY_BYTES: u64 = 2 * 1024 * 1024;
const RERANK_MAXIMUM_METADATA_BYTES: u64 = 64 * 1024;

pub(super) fn execute_query(
    host: &HostExecutor,
    query_id: pod0_facade::RecallQueryId,
    provider: RecallEmbeddingProvider,
    model: String,
    text: String,
    maximum_dimensions: u16,
) -> HostObservation {
    let endpoint = match recall_embedding_endpoint(host, provider, &model) {
        Ok(endpoint) => endpoint,
        Err(observation) => return *observation,
    };
    let request = EmbeddingRequest {
        model,
        inputs: vec![text],
        dimensions: Some(u32::from(maximum_dimensions)),
        timeout: EMBEDDING_TIMEOUT,
        limits: HttpLimits {
            maximum_body_bytes: EMBEDDING_MAXIMUM_BODY_BYTES,
            maximum_metadata_bytes: EMBEDDING_MAXIMUM_METADATA_BYTES,
        },
        maximum_dimensions: u32::from(maximum_dimensions),
        maximum_vectors: 1,
    };
    let cancellation = CancellationToken::new();
    let result = match provider {
        RecallEmbeddingProvider::OpenRouter => host.runtime.block_on(host.live.openai_embeddings(
            OpenAiEmbeddingRequest {
                endpoint,
                embedding: request,
            },
            &cancellation,
        )),
        RecallEmbeddingProvider::Ollama => host.runtime.block_on(host.live.ollama_embeddings(
            OllamaEmbeddingRequest {
                endpoint,
                embedding: request,
            },
            &cancellation,
        )),
        RecallEmbeddingProvider::Unsupported { wire_code } => {
            return failed(
                HostFailureCode::Unsupported { wire_code },
                "recall embedding provider is unsupported",
            );
        }
    };
    match result {
        Ok(response) => match response.embeddings.into_iter().next() {
            Some(embedding) => {
                if embedding.values.is_empty() {
                    return failed(
                        HostFailureCode::InvalidResponse,
                        "provider returned an empty embedding",
                    );
                }
                HostObservation::RecallQueryEmbedded {
                    query_id,
                    embedding: RecallEmbeddingVector {
                        values: quantize(embedding.values),
                    },
                }
            }
            None => failed(
                HostFailureCode::InvalidResponse,
                "provider returned no embedding",
            ),
        },
        Err(error) => recall_failure(error),
    }
}

pub(super) fn execute_spans(
    host: &HostExecutor,
    episode_id: pod0_facade::EpisodeId,
    generation_id: EvidenceGenerationId,
    provider: RecallEmbeddingProvider,
    model: String,
    spans: Vec<RecallEmbeddingInput>,
    maximum_dimensions: u16,
) -> HostObservation {
    let endpoint = match recall_embedding_endpoint(host, provider, &model) {
        Ok(endpoint) => endpoint,
        Err(observation) => return *observation,
    };
    let inputs: Vec<String> = spans.iter().map(|span| span.text.clone()).collect();
    let span_ids: Vec<_> = spans.iter().map(|span| span.span_id).collect();
    let maximum_vectors = u32::try_from(inputs.len()).unwrap_or(u32::MAX);
    let request = EmbeddingRequest {
        model,
        inputs,
        dimensions: Some(u32::from(maximum_dimensions)),
        timeout: EMBEDDING_TIMEOUT,
        limits: HttpLimits {
            maximum_body_bytes: EMBEDDING_MAXIMUM_BODY_BYTES,
            maximum_metadata_bytes: EMBEDDING_MAXIMUM_METADATA_BYTES,
        },
        maximum_dimensions: u32::from(maximum_dimensions),
        maximum_vectors,
    };
    let cancellation = CancellationToken::new();
    let result = match provider {
        RecallEmbeddingProvider::OpenRouter => host.runtime.block_on(host.live.openai_embeddings(
            OpenAiEmbeddingRequest {
                endpoint,
                embedding: request,
            },
            &cancellation,
        )),
        RecallEmbeddingProvider::Ollama => host.runtime.block_on(host.live.ollama_embeddings(
            OllamaEmbeddingRequest {
                endpoint,
                embedding: request,
            },
            &cancellation,
        )),
        RecallEmbeddingProvider::Unsupported { wire_code } => {
            return failed(
                HostFailureCode::Unsupported { wire_code },
                "recall embedding provider is unsupported",
            );
        }
    };
    match result {
        Ok(response) => {
            if response.embeddings.len() != span_ids.len() {
                return failed(
                    HostFailureCode::InvalidResponse,
                    "provider returned the wrong number of span embeddings",
                );
            }
            let embeddings = response
                .embeddings
                .into_iter()
                .zip(span_ids)
                .map(|(embedding, span_id)| RecallSpanEmbeddingObservation {
                    span_id,
                    embedding: RecallEmbeddingVector {
                        values: quantize(embedding.values),
                    },
                })
                .collect();
            HostObservation::RecallSpansEmbedded {
                episode_id,
                generation_id,
                embeddings,
            }
        }
        Err(error) => recall_failure(error),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub(super) fn execute_rerank(
    host: &HostExecutor,
    query_id: pod0_facade::RecallQueryId,
    provider: RecallRerankProvider,
    model: String,
    query: String,
    candidates: Vec<RecallRerankDocument>,
) -> HostObservation {
    let endpoint = match recall_rerank_endpoint(host, provider, &model) {
        Ok(endpoint) => endpoint,
        Err(observation) => return *observation,
    };
    let documents = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| RerankDocument {
            id: format!("{index}"),
            text: candidate.excerpt.clone(),
        })
        .collect::<Vec<_>>();
    let maximum_results = u32::try_from(candidates.len()).unwrap_or(u32::MAX).min(20);
    let request = RerankRequest {
        endpoint,
        model,
        query,
        documents,
        top_n: Some(maximum_results),
        timeout: EMBEDDING_TIMEOUT,
        limits: HttpLimits {
            maximum_body_bytes: RERANK_MAXIMUM_BODY_BYTES,
            maximum_metadata_bytes: RERANK_MAXIMUM_METADATA_BYTES,
        },
        maximum_results,
    };
    let cancellation = CancellationToken::new();
    let result = host
        .runtime
        .block_on(host.live.rerank(request, &cancellation));
    match result {
        Ok(response) => {
            let rankings = response
                .results
                .into_iter()
                .enumerate()
                .filter_map(|(position, result)| {
                    let index = usize::try_from(result.index).ok()?;
                    let document = candidates.get(index)?;
                    Some(RecallRerankObservation {
                        span_id: document.span_id,
                        rank: u16::try_from(position + 1).unwrap_or(u16::MAX),
                    })
                })
                .collect::<Vec<_>>();
            if rankings.is_empty() {
                return failed(
                    HostFailureCode::InvalidResponse,
                    "rerank returned no candidate rankings",
                );
            }
            HostObservation::RecallCandidatesReranked { query_id, rankings }
        }
        Err(error) => recall_failure(error),
    }
}
