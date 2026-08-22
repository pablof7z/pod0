use serde_json::Value;

use crate::{
    AdapterError, Embedding, EmbeddingResponse, HttpEvidence, SizeError, SizeSubject, TokenUsage,
    provider::invalid_provider_response,
};

pub(crate) fn parse_openai(
    bytes: &[u8],
    evidence: HttpEvidence,
    input_count: usize,
    requested_dimensions: Option<u32>,
    maximum_dimensions: u32,
    maximum_vectors: u32,
) -> Result<EmbeddingResponse, AdapterError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| invalid_provider_response("embedding JSON"))?;
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_provider_response("embedding data"))?;
    enforce_vector_count(data.len(), input_count, maximum_vectors)?;
    let mut embeddings = data
        .iter()
        .map(|item| {
            let index = item
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or_else(|| invalid_provider_response("embedding index"))?;
            let values = parse_vector(
                item.get("embedding"),
                requested_dimensions,
                maximum_dimensions,
            )?;
            Ok(Embedding { index, values })
        })
        .collect::<Result<Vec<_>, AdapterError>>()?;
    embeddings.sort_by_key(|embedding| embedding.index);
    if embeddings
        .iter()
        .enumerate()
        .any(|(index, embedding)| usize::try_from(embedding.index).ok() != Some(index))
    {
        return Err(invalid_provider_response("embedding indices"));
    }
    enforce_consistent_dimensions(&embeddings)?;
    Ok(EmbeddingResponse {
        embeddings,
        usage: parse_openai_usage(value.get("usage"))?,
        model: optional_string(&value, "model")?,
        evidence,
    })
}

pub(crate) fn parse_ollama(
    bytes: &[u8],
    evidence: HttpEvidence,
    input_count: usize,
    requested_dimensions: Option<u32>,
    maximum_dimensions: u32,
    maximum_vectors: u32,
) -> Result<EmbeddingResponse, AdapterError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| invalid_provider_response("Ollama embedding JSON"))?;
    let vectors = value
        .get("embeddings")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_provider_response("Ollama embeddings"))?;
    enforce_vector_count(vectors.len(), input_count, maximum_vectors)?;
    let embeddings = vectors
        .iter()
        .enumerate()
        .map(|(index, value)| {
            Ok(Embedding {
                index: u32::try_from(index)
                    .map_err(|_| invalid_provider_response("embedding index"))?,
                values: parse_vector(Some(value), requested_dimensions, maximum_dimensions)?,
            })
        })
        .collect::<Result<Vec<_>, AdapterError>>()?;
    enforce_consistent_dimensions(&embeddings)?;
    Ok(EmbeddingResponse {
        embeddings,
        usage: TokenUsage {
            prompt_tokens: optional_u64(&value, "prompt_eval_count")?,
            ..TokenUsage::default()
        },
        model: optional_string(&value, "model")?,
        evidence,
    })
}

pub(crate) fn parse_vector(
    value: Option<&Value>,
    requested_dimensions: Option<u32>,
    maximum_dimensions: u32,
) -> Result<Vec<f32>, AdapterError> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_provider_response("embedding vector"))?;
    let dimensions = u32::try_from(values.len()).unwrap_or(u32::MAX);
    if values.is_empty() || dimensions > maximum_dimensions {
        return Err(AdapterError::Size(SizeError {
            subject: SizeSubject::EmbeddingDimensions,
            limit: u64::from(maximum_dimensions),
            observed: Some(u64::from(dimensions)),
        }));
    }
    if requested_dimensions.is_some_and(|requested| dimensions != requested) {
        return Err(invalid_provider_response("embedding dimensions"));
    }
    values
        .iter()
        .map(|value| {
            value
                .as_f64()
                .filter(|value| {
                    value.is_finite()
                        && *value >= f64::from(f32::MIN)
                        && *value <= f64::from(f32::MAX)
                })
                .map(|value| value as f32)
                .ok_or_else(|| invalid_provider_response("embedding value"))
        })
        .collect()
}

fn enforce_consistent_dimensions(embeddings: &[Embedding]) -> Result<(), AdapterError> {
    let dimensions = embeddings
        .first()
        .map(|embedding| embedding.values.len())
        .ok_or_else(|| invalid_provider_response("embedding count"))?;
    if embeddings
        .iter()
        .any(|embedding| embedding.values.len() != dimensions)
    {
        return Err(invalid_provider_response("embedding dimensions"));
    }
    Ok(())
}

fn enforce_vector_count(
    observed: usize,
    expected: usize,
    maximum: u32,
) -> Result<(), AdapterError> {
    let observed = u32::try_from(observed).unwrap_or(u32::MAX);
    if observed > maximum {
        return Err(AdapterError::Size(SizeError {
            subject: SizeSubject::ResultCount,
            limit: u64::from(maximum),
            observed: Some(u64::from(observed)),
        }));
    }
    if usize::try_from(observed).ok() != Some(expected) {
        return Err(invalid_provider_response("embedding count"));
    }
    Ok(())
}

fn parse_openai_usage(value: Option<&Value>) -> Result<TokenUsage, AdapterError> {
    Ok(TokenUsage {
        prompt_tokens: value
            .and_then(|usage| usage.get("prompt_tokens"))
            .map(parse_u64)
            .transpose()?,
        total_tokens: value
            .and_then(|usage| usage.get("total_tokens"))
            .map(parse_u64)
            .transpose()?,
        ..TokenUsage::default()
    })
}

fn optional_u64(value: &Value, key: &'static str) -> Result<Option<u64>, AdapterError> {
    value.get(key).map(parse_u64).transpose()
}

fn parse_u64(value: &Value) -> Result<u64, AdapterError> {
    value
        .as_u64()
        .ok_or_else(|| invalid_provider_response("embedding usage"))
}

fn optional_string(value: &Value, key: &'static str) -> Result<Option<String>, AdapterError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(invalid_provider_response(key)),
    }
}
