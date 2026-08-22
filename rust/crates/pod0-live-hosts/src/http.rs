use std::{fmt, time::Duration};

use reqwest::header::{ACCEPT, IF_MODIFIED_SINCE, IF_NONE_MATCH, LOCATION};

use crate::{
    AdapterError, CancellationToken, LiveHosts, ProtocolError, SizeError, SizeSubject,
    bounds::{bounded_body, bounded_header, metadata_size},
    url_debug::RedactedUrl,
};

#[derive(Clone, Copy, Debug)]
pub struct HttpLimits {
    pub maximum_body_bytes: u64,
    pub maximum_metadata_bytes: u64,
}

impl HttpLimits {
    pub(crate) fn validate(self) -> Result<(), AdapterError> {
        if self.maximum_body_bytes == 0 || self.maximum_metadata_bytes == 0 {
            return Err(AdapterError::Protocol(ProtocolError {
                context: "request limits",
                status: None,
                evidence: None,
            }));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RequestOptions {
    pub timeout: Duration,
    pub maximum_redirects: u8,
    pub limits: HttpLimits,
}

#[derive(Clone, Debug)]
pub struct HttpGetRequest {
    pub url: String,
    pub accept: Option<String>,
    pub entity_tag: Option<String>,
    pub last_modified: Option<String>,
    pub options: RequestOptions,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RedirectEvidence {
    pub status: u16,
    pub location: String,
    pub resolved_url: String,
}

impl fmt::Debug for RedirectEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedirectEvidence")
            .field("status", &self.status)
            .field("location", &RedactedUrl::new(&self.location))
            .field("resolved_url", &RedactedUrl::new(&self.resolved_url))
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HttpEvidence {
    pub status: u16,
    pub final_url: String,
    pub redirects: Vec<RedirectEvidence>,
    pub entity_tag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
}

impl fmt::Debug for HttpEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpEvidence")
            .field("status", &self.status)
            .field("final_url", &RedactedUrl::new(&self.final_url))
            .field("redirects", &self.redirects)
            .field("entity_tag", &self.entity_tag)
            .field("last_modified", &self.last_modified)
            .field("content_type", &self.content_type)
            .field("content_length", &self.content_length)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpGetResponse {
    pub evidence: HttpEvidence,
    pub body: Vec<u8>,
}

impl LiveHosts {
    #[tracing::instrument(
        skip(self, request, cancellation),
        fields(status = tracing::field::Empty, duration_ms = tracing::field::Empty, outcome = tracing::field::Empty)
    )]
    pub async fn http_get(
        &self,
        request: HttpGetRequest,
        cancellation: &CancellationToken,
    ) -> Result<HttpGetResponse, AdapterError> {
        cancellation.check()?;
        request.options.limits.validate()?;
        let started = std::time::Instant::now();
        let result = self
            .run(request.options.timeout, cancellation, async {
                let (response, evidence) = self
                    .follow_get(
                        &request.url,
                        request.accept.as_deref(),
                        request.entity_tag.as_deref(),
                        request.last_modified.as_deref(),
                        request.options,
                    )
                    .await?;
                let body =
                    bounded_body(response, request.options.limits.maximum_body_bytes).await?;
                Ok(HttpGetResponse { evidence, body })
            })
            .await;
        crate::tracing_support::record_outcome(started, &result, |response| {
            response.evidence.status
        });
        result
    }

    pub(crate) async fn follow_get(
        &self,
        url: &str,
        accept: Option<&str>,
        entity_tag: Option<&str>,
        last_modified: Option<&str>,
        options: RequestOptions,
    ) -> Result<(reqwest::Response, HttpEvidence), AdapterError> {
        let mut current = self.parse_url(url, "HTTP GET URL")?;
        let mut redirects = Vec::new();
        loop {
            let mut request = self.client.get(current.clone());
            if let Some(value) = accept {
                request = request.header(ACCEPT, value);
            }
            if let Some(value) = entity_tag {
                request = request.header(IF_NONE_MATCH, value);
            }
            if let Some(value) = last_modified {
                request = request.header(IF_MODIFIED_SINCE, value);
            }
            let response = request
                .send()
                .await
                .map_err(|error| AdapterError::from_reqwest(&error))?;
            if is_follow_redirect(response.status()) {
                if redirects.len() >= usize::from(options.maximum_redirects) {
                    return Err(AdapterError::Protocol(ProtocolError {
                        context: "redirect limit",
                        status: Some(response.status().as_u16()),
                        evidence: None,
                    }));
                }
                let location = bounded_header(
                    response.headers(),
                    LOCATION,
                    options.limits.maximum_metadata_bytes,
                )?
                .ok_or(AdapterError::Protocol(ProtocolError {
                    context: "redirect location",
                    status: Some(response.status().as_u16()),
                    evidence: None,
                }))?;
                let resolved = current.join(&location).map_err(|_| {
                    AdapterError::Protocol(ProtocolError {
                        context: "redirect location",
                        status: Some(response.status().as_u16()),
                        evidence: None,
                    })
                })?;
                self.validate_scheme(&resolved, "redirect URL")?;
                redirects.push(RedirectEvidence {
                    status: response.status().as_u16(),
                    location,
                    resolved_url: resolved.to_string(),
                });
                let observed = redirect_metadata_size(&redirects, resolved.as_str());
                if observed > options.limits.maximum_metadata_bytes {
                    return Err(AdapterError::Size(SizeError {
                        subject: SizeSubject::Metadata,
                        limit: options.limits.maximum_metadata_bytes,
                        observed: Some(observed),
                    }));
                }
                current = resolved;
                continue;
            }
            let evidence = self.evidence(&response, redirects, options.limits)?;
            return Ok((response, evidence));
        }
    }

    pub(crate) fn evidence(
        &self,
        response: &reqwest::Response,
        redirects: Vec<RedirectEvidence>,
        limits: HttpLimits,
    ) -> Result<HttpEvidence, AdapterError> {
        let headers = response.headers();
        let evidence = HttpEvidence {
            status: response.status().as_u16(),
            final_url: response.url().to_string(),
            redirects,
            entity_tag: bounded_header(
                headers,
                reqwest::header::ETAG,
                limits.maximum_metadata_bytes,
            )?,
            last_modified: bounded_header(
                headers,
                reqwest::header::LAST_MODIFIED,
                limits.maximum_metadata_bytes,
            )?,
            content_type: bounded_header(
                headers,
                reqwest::header::CONTENT_TYPE,
                limits.maximum_metadata_bytes,
            )?,
            content_length: response.content_length(),
        };
        let observed = metadata_size(&evidence);
        if observed > limits.maximum_metadata_bytes {
            return Err(AdapterError::Size(SizeError {
                subject: SizeSubject::Metadata,
                limit: limits.maximum_metadata_bytes,
                observed: Some(observed),
            }));
        }
        Ok(evidence)
    }
}

fn is_follow_redirect(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

fn redirect_metadata_size(redirects: &[RedirectEvidence], final_url: &str) -> u64 {
    redirects.iter().fold(
        u64::try_from(final_url.len()).unwrap_or(u64::MAX),
        |size, redirect| {
            size.saturating_add(u64::try_from(redirect.location.len()).unwrap_or(u64::MAX))
                .saturating_add(u64::try_from(redirect.resolved_url.len()).unwrap_or(u64::MAX))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_limits_are_rejected() {
        let error = HttpLimits {
            maximum_body_bytes: 0,
            maximum_metadata_bytes: 1,
        }
        .validate()
        .expect_err("zero body limit must fail");
        assert!(matches!(error, AdapterError::Protocol(_)));
    }

    #[test]
    fn not_modified_is_evidence_not_a_redirect() {
        assert!(!is_follow_redirect(reqwest::StatusCode::NOT_MODIFIED));
    }
}
