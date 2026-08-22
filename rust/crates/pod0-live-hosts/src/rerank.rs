use std::time::Duration;

use serde_json::{Value, json};

use crate::{
    AdapterError, CancellationToken, HttpEvidence, HttpLimits, LiveHosts, ProviderEndpoint,
    ProviderKind, SizeError, SizeSubject, bounds::bounded_body,
    provider::invalid_provider_response,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RerankDocument {
    pub id: String,
    pub text: String,
}

#[derive(Debug)]
pub struct RerankRequest {
    pub endpoint: ProviderEndpoint,
    pub model: String,
    pub query: String,
    pub documents: Vec<RerankDocument>,
    pub top_n: Option<u32>,
    pub timeout: Duration,
    pub limits: HttpLimits,
    pub maximum_results: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RerankResult {
    pub index: u32,
    pub document_id: String,
    pub relevance_score: f64,
    pub provider_document: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RerankResponse {
    pub results: Vec<RerankResult>,
    pub response_id: Option<String>,
    pub billed_search_units: Option<u64>,
    pub evidence: HttpEvidence,
}

impl LiveHosts {
    pub async fn rerank(
        &self,
        request: RerankRequest,
        cancellation: &CancellationToken,
    ) -> Result<RerankResponse, AdapterError> {
        cancellation.check()?;
        validate_request(&request)?;
        let response_limit = request
            .top_n
            .unwrap_or(request.maximum_results)
            .min(request.maximum_results);
        let texts = request
            .documents
            .iter()
            .map(|document| document.text.clone())
            .collect::<Vec<_>>();
        let mut body = json!({
            "model": request.model,
            "query": request.query,
            "documents": texts,
            "return_documents": false
        });
        if let Some(top_n) = request.top_n {
            body["top_n"] = json!(top_n);
        }
        self.run(request.timeout, cancellation, async {
            let response = self
                .provider_request(&request.endpoint, ProviderKind::Rerank)?
                .json(&body)
                .send()
                .await
                .map_err(|error| AdapterError::from_reqwest(&error))?;
            let (response, evidence) = self
                .provider_response(response, ProviderKind::Rerank, request.limits)
                .await?;
            let bytes = bounded_body(response, request.limits.maximum_body_bytes).await?;
            parse_response(&bytes, evidence, &request.documents, response_limit)
        })
        .await
    }
}

fn validate_request(request: &RerankRequest) -> Result<(), AdapterError> {
    request.limits.validate()?;
    let count = u32::try_from(request.documents.len()).unwrap_or(u32::MAX);
    if request.model.trim().is_empty()
        || request.query.is_empty()
        || request.documents.is_empty()
        || request
            .documents
            .iter()
            .any(|document| document.id.is_empty() || document.text.is_empty())
        || request.maximum_results == 0
        || request.top_n == Some(0)
        || request.top_n.is_some_and(|top_n| top_n > count)
    {
        return Err(invalid_provider_response("rerank request"));
    }
    Ok(())
}

fn parse_response(
    bytes: &[u8],
    evidence: HttpEvidence,
    documents: &[RerankDocument],
    maximum_results: u32,
) -> Result<RerankResponse, AdapterError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| invalid_provider_response("rerank JSON"))?;
    let values = value
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_provider_response("rerank results"))?;
    let observed = u32::try_from(values.len()).unwrap_or(u32::MAX);
    if observed > maximum_results {
        return Err(AdapterError::Size(SizeError {
            subject: SizeSubject::ResultCount,
            limit: u64::from(maximum_results),
            observed: Some(u64::from(observed)),
        }));
    }
    let mut seen = vec![false; documents.len()];
    let results = values
        .iter()
        .map(|result| {
            let index = result
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok())
                .filter(|index| *index < documents.len())
                .ok_or_else(|| invalid_provider_response("rerank result index"))?;
            if seen[index] {
                return Err(invalid_provider_response("rerank duplicate index"));
            }
            seen[index] = true;
            let score = result
                .get("relevance_score")
                .and_then(Value::as_f64)
                .filter(|score| score.is_finite())
                .ok_or_else(|| invalid_provider_response("rerank relevance score"))?;
            Ok(RerankResult {
                index: u32::try_from(index)
                    .map_err(|_| invalid_provider_response("rerank result index"))?,
                document_id: documents[index].id.clone(),
                relevance_score: score,
                provider_document: result.get("document").cloned(),
            })
        })
        .collect::<Result<Vec<_>, AdapterError>>()?;
    Ok(RerankResponse {
        results,
        response_id: optional_string(&value, "id")?,
        billed_search_units: value
            .pointer("/meta/billed_units/search_units")
            .map(|value| {
                value
                    .as_u64()
                    .ok_or_else(|| invalid_provider_response("rerank billed units"))
            })
            .transpose()?,
        evidence,
    })
}

fn optional_string(value: &Value, key: &'static str) -> Result<Option<String>, AdapterError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(invalid_provider_response(key)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_result_indices_are_rejected() {
        let documents = vec![RerankDocument {
            id: "one".to_owned(),
            text: "document".to_owned(),
        }];
        let evidence = HttpEvidence {
            status: 200,
            final_url: "https://example.invalid/rerank".to_owned(),
            redirects: Vec::new(),
            entity_tag: None,
            last_modified: None,
            content_type: None,
            content_length: None,
        };
        assert!(
            parse_response(
                br#"{"results":[{"index":0,"relevance_score":1},{"index":0,"relevance_score":0}]}"#,
                evidence,
                &documents,
                2
            )
            .is_err()
        );
    }

    #[test]
    fn response_count_obeys_top_n_below_maximum_results() {
        let documents = vec![
            RerankDocument {
                id: "one".to_owned(),
                text: "first".to_owned(),
            },
            RerankDocument {
                id: "two".to_owned(),
                text: "second".to_owned(),
            },
        ];
        let evidence = HttpEvidence {
            status: 200,
            final_url: "https://example.invalid/rerank".to_owned(),
            redirects: Vec::new(),
            entity_tag: None,
            last_modified: None,
            content_type: None,
            content_length: None,
        };
        assert!(matches!(
            parse_response(
                br#"{"results":[{"index":0,"relevance_score":1},{"index":1,"relevance_score":0}]}"#,
                evidence,
                &documents,
                1
            ),
            Err(AdapterError::Size(SizeError {
                subject: SizeSubject::ResultCount,
                limit: 1,
                ..
            }))
        ));
    }
}
