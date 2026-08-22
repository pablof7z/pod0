use std::fmt;

use reqwest::header::{AUTHORIZATION, HeaderValue, RETRY_AFTER};
use zeroize::Zeroize as _;

use crate::{
    AdapterError, CredentialError, HttpEvidence, HttpLimits, LiveHosts, ProtocolError,
    SecretString,
    bounds::{bounded_error_body, bounded_header},
    url_debug::RedactedUrl,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAiCompatible,
    Ollama,
    Transcription,
    Embeddings,
    Rerank,
}

impl ProviderKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI-compatible",
            Self::Ollama => "Ollama",
            Self::Transcription => "transcription",
            Self::Embeddings => "embeddings",
            Self::Rerank => "rerank",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialRequirement {
    Optional,
    Required,
}

pub struct ProviderEndpoint {
    pub url: String,
    pub bearer_token: Option<SecretString>,
    pub credential_requirement: CredentialRequirement,
}

impl ProviderEndpoint {
    #[must_use]
    pub fn unauthenticated(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            bearer_token: None,
            credential_requirement: CredentialRequirement::Optional,
        }
    }

    #[must_use]
    pub fn bearer(url: impl Into<String>, token: SecretString) -> Self {
        Self {
            url: url.into(),
            bearer_token: Some(token),
            credential_requirement: CredentialRequirement::Required,
        }
    }

    #[must_use]
    pub fn requiring_bearer(url: impl Into<String>, token: Option<SecretString>) -> Self {
        Self {
            url: url.into(),
            bearer_token: token,
            credential_requirement: CredentialRequirement::Required,
        }
    }
}

impl fmt::Debug for ProviderEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderEndpoint")
            .field("url", &RedactedUrl::new(&self.url))
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("credential_requirement", &self.credential_requirement)
            .finish()
    }
}

impl LiveHosts {
    pub(crate) fn provider_request(
        &self,
        endpoint: &ProviderEndpoint,
        provider: ProviderKind,
    ) -> Result<reqwest::RequestBuilder, AdapterError> {
        let url = self.parse_url(&endpoint.url, "provider endpoint")?;
        if endpoint.credential_requirement == CredentialRequirement::Required
            && endpoint.bearer_token.is_none()
        {
            return Err(AdapterError::Credential(CredentialError {
                provider,
                reason: "required bearer token was not supplied",
            }));
        }
        let mut request = self.client.post(url);
        if let Some(token) = &endpoint.bearer_token {
            let mut bytes = Vec::with_capacity(7 + token.expose().len());
            bytes.extend_from_slice(b"Bearer ");
            bytes.extend_from_slice(token.expose().as_bytes());
            let value = HeaderValue::from_bytes(&bytes);
            bytes.zeroize();
            let mut value = value.map_err(|_| {
                AdapterError::Credential(CredentialError {
                    provider,
                    reason: "bearer token is not a valid HTTP header value",
                })
            })?;
            value.set_sensitive(true);
            request = request.header(AUTHORIZATION, value);
        }
        Ok(request)
    }

    pub(crate) async fn provider_response(
        &self,
        response: reqwest::Response,
        provider: ProviderKind,
        limits: HttpLimits,
    ) -> Result<(reqwest::Response, HttpEvidence), AdapterError> {
        let evidence = self.evidence(&response, Vec::new(), limits)?;
        if response.status().is_success() {
            return Ok((response, evidence));
        }
        let status = response.status().as_u16();
        let retry_after = bounded_header(
            response.headers(),
            RETRY_AFTER,
            limits.maximum_metadata_bytes,
        )?;
        let request_id = provider_request_id(response.headers(), limits.maximum_metadata_bytes)?;
        let (body, body_truncated) =
            bounded_error_body(response, limits.maximum_body_bytes).await?;
        Err(AdapterError::Provider(crate::ProviderError {
            provider,
            status,
            retry_after,
            request_id,
            body,
            body_truncated,
        }))
    }
}

fn provider_request_id(
    headers: &reqwest::header::HeaderMap,
    limit: u64,
) -> Result<Option<String>, AdapterError> {
    for name in ["x-request-id", "request-id", "x-correlation-id"] {
        let name = reqwest::header::HeaderName::from_static(name);
        if let Some(value) = bounded_header(headers, name, limit)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub(crate) fn invalid_provider_response(context: &'static str) -> AdapterError {
    AdapterError::Protocol(ProtocolError {
        context,
        status: None,
        evidence: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_debug_redacts_bearer_token() {
        let endpoint = ProviderEndpoint::bearer(
            "https://alice:url-password@example.invalid/v1?access_key=query-secret&view=full",
            SecretString::new("provider-secret"),
        );
        let output = format!("{endpoint:?}");
        assert!(!output.contains("provider-secret"));
        assert!(!output.contains("alice"));
        assert!(!output.contains("url-password"));
        assert!(!output.contains("query-secret"));
        assert!(output.contains("view=full"));
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn invalid_endpoint_debug_does_not_echo_input() {
        let endpoint = ProviderEndpoint::unauthenticated("not a URL with secret-token");
        assert!(!format!("{endpoint:?}").contains("secret-token"));
    }
}
