use futures_util::StreamExt as _;
use reqwest::{
    Response,
    header::{HeaderMap, HeaderName},
};

use crate::{AdapterError, HttpEvidence, SizeError, SizeSubject};

pub(crate) async fn bounded_body(response: Response, limit: u64) -> Result<Vec<u8>, AdapterError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err(size_error(
            SizeSubject::ResponseBody,
            limit,
            response.content_length(),
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AdapterError::response_body())?;
        let next = u64::try_from(body.len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        if next > limit {
            return Err(size_error(SizeSubject::ResponseBody, limit, Some(next)));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub(crate) async fn bounded_error_body(
    response: Response,
    limit: u64,
) -> Result<(Vec<u8>, bool), AdapterError> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AdapterError::response_body())?;
        let remaining = usize::try_from(limit)
            .unwrap_or(usize::MAX)
            .saturating_sub(body.len());
        if chunk.len() > remaining {
            body.extend_from_slice(&chunk[..remaining]);
            return Ok((body, true));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((body, false))
}

pub(crate) fn bounded_header(
    headers: &HeaderMap,
    name: HeaderName,
    limit: u64,
) -> Result<Option<String>, AdapterError> {
    let Some(value) = headers.get(name) else {
        return Ok(None);
    };
    let bytes = value.as_bytes();
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(size_error(
            SizeSubject::Metadata,
            limit,
            Some(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
        ));
    }
    let text = value.to_str().map_err(|_| {
        AdapterError::Protocol(crate::ProtocolError {
            context: "HTTP header",
            status: None,
            evidence: None,
        })
    })?;
    Ok(Some(text.to_owned()))
}

pub(crate) fn metadata_size(evidence: &HttpEvidence) -> u64 {
    let mut size = evidence.final_url.len();
    for value in [
        evidence.entity_tag.as_deref(),
        evidence.last_modified.as_deref(),
        evidence.content_type.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        size = size.saturating_add(value.len());
    }
    for redirect in &evidence.redirects {
        size = size
            .saturating_add(redirect.location.len())
            .saturating_add(redirect.resolved_url.len());
    }
    u64::try_from(size).unwrap_or(u64::MAX)
}

pub(crate) fn enforce_output(size: usize, limit: u64) -> Result<(), AdapterError> {
    let observed = u64::try_from(size).unwrap_or(u64::MAX);
    if observed > limit {
        return Err(size_error(SizeSubject::Output, limit, Some(observed)));
    }
    Ok(())
}

pub(crate) fn size_error(subject: SizeSubject, limit: u64, observed: Option<u64>) -> AdapterError {
    AdapterError::Size(SizeError {
        subject,
        limit,
        observed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_bound_is_strict() {
        assert!(enforce_output(4, 4).is_ok());
        assert!(matches!(
            enforce_output(5, 4),
            Err(AdapterError::Size(SizeError {
                subject: SizeSubject::Output,
                limit: 4,
                observed: Some(5)
            }))
        ));
    }
}
