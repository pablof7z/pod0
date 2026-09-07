use super::*;

pub(super) fn recall_embedding_endpoint(
    host: &HostExecutor,
    provider: RecallEmbeddingProvider,
    model: &str,
) -> Result<ProviderEndpoint, Box<HostObservation>> {
    if model.trim().is_empty() {
        return Err(Box::new(failed(
            HostFailureCode::InvalidResponse,
            "recall embedding model is empty",
        )));
    }
    match provider {
        RecallEmbeddingProvider::OpenRouter => {
            let Some(base) = host.config.openai_base_url.as_deref() else {
                return Err(Box::new(failed(
                    HostFailureCode::ProviderUnavailable,
                    "OpenAI-compatible embeddings endpoint is not configured",
                )));
            };
            Ok(ProviderEndpoint {
                url: format!("{}/embeddings", base.trim_end_matches('/')),
                bearer_token: host
                    .config
                    .openai_api_key
                    .as_ref()
                    .map(|key| SecretString::new(key.clone())),
                credential_requirement: CredentialRequirement::Optional,
            })
        }
        RecallEmbeddingProvider::Ollama => {
            let Some(base) = host.config.ollama_base_url.as_deref() else {
                return Err(Box::new(failed(
                    HostFailureCode::ProviderUnavailable,
                    "Ollama embeddings endpoint is not configured",
                )));
            };
            Ok(ProviderEndpoint {
                url: format!("{}/api/embeddings", base.trim_end_matches('/')),
                bearer_token: None,
                credential_requirement: CredentialRequirement::Optional,
            })
        }
        RecallEmbeddingProvider::Unsupported { wire_code } => Err(Box::new(failed(
            HostFailureCode::Unsupported { wire_code },
            "recall embedding provider is unsupported",
        ))),
    }
}

pub(super) fn recall_rerank_endpoint(
    host: &HostExecutor,
    provider: RecallRerankProvider,
    model: &str,
) -> Result<ProviderEndpoint, Box<HostObservation>> {
    if model.trim().is_empty() {
        return Err(Box::new(failed(
            HostFailureCode::InvalidResponse,
            "recall rerank model is empty",
        )));
    }
    match provider {
        RecallRerankProvider::OpenRouter => {
            let Some(base) = host.config.openai_base_url.as_deref() else {
                return Err(Box::new(failed(
                    HostFailureCode::ProviderUnavailable,
                    "OpenAI-compatible rerank endpoint is not configured",
                )));
            };
            Ok(ProviderEndpoint {
                url: format!("{}/rerank", base.trim_end_matches('/')),
                bearer_token: host
                    .config
                    .openai_api_key
                    .as_ref()
                    .map(|key| SecretString::new(key.clone())),
                credential_requirement: CredentialRequirement::Optional,
            })
        }
        RecallRerankProvider::Unsupported { wire_code } => Err(Box::new(failed(
            HostFailureCode::Unsupported { wire_code },
            "recall rerank provider is unsupported",
        ))),
    }
}

pub(super) fn quantize(values: Vec<f32>) -> Vec<i32> {
    values
        .into_iter()
        .map(|value| {
            let scaled = value * MILLIONTHS;
            if scaled.is_nan() {
                0
            } else if scaled >= i32::MAX as f32 {
                i32::MAX
            } else if scaled <= i32::MIN as f32 {
                i32::MIN
            } else {
                scaled.round() as i32
            }
        })
        .collect()
}

pub(super) fn recall_failure(error: AdapterError) -> HostObservation {
    failed(recall_failure_code(&error), &recall_failure_detail(&error))
}

fn recall_failure_code(error: &AdapterError) -> HostFailureCode {
    match error {
        AdapterError::Network(network) => match network.kind {
            pod0_live_hosts::NetworkErrorKind::Timeout => HostFailureCode::TimedOut,
            _ => HostFailureCode::Offline,
        },
        AdapterError::Credential(_) | AdapterError::Provider(_) => HostFailureCode::Unauthorized,
        AdapterError::Size(_) => HostFailureCode::ResponseTooLarge,
        AdapterError::Unavailable(_) => HostFailureCode::ProviderUnavailable,
        AdapterError::Protocol(_) | AdapterError::File(_) => HostFailureCode::InvalidResponse,
        AdapterError::Cancelled => HostFailureCode::TimedOut,
        _ => HostFailureCode::InvalidResponse,
    }
}

fn recall_failure_detail(error: &AdapterError) -> String {
    let capacity = 512;
    let detail = format!("{error:?}");
    detail.chars().take(capacity).collect()
}

#[cfg(test)]
mod tests {
    use super::quantize;

    #[test]
    fn quantize_clamps_to_signed_millionths() {
        assert_eq!(
            quantize(vec![0.0, 0.5, -0.25, 1.0, -1.0]),
            vec![0, 500_000, -250_000, 1_000_000, -1_000_000]
        );
        assert_eq!(quantize(vec![f32::INFINITY]), vec![i32::MAX]);
        assert_eq!(quantize(vec![f32::NEG_INFINITY]), vec![i32::MIN]);
        assert_eq!(quantize(vec![f32::NAN]), vec![0]);
    }

    #[test]
    fn quantize_preserves_dimension_count() {
        assert_eq!(quantize(vec![0.1; 8]).len(), 8);
    }
}
