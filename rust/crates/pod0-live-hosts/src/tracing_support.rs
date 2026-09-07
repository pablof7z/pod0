use std::time::Instant;

use crate::{AdapterError, NetworkErrorKind};

/// Records `status`/`duration_ms`/`outcome` on the current span for an
/// instrumented call — never the request/response body or a bearer token,
/// since those are never turned into span fields anywhere in this module.
pub(crate) fn record_outcome<T>(
    started: Instant,
    result: &Result<T, AdapterError>,
    status: impl FnOnce(&T) -> u16,
) {
    let span = tracing::Span::current();
    span.record(
        "duration_ms",
        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    );
    match result {
        Ok(value) => {
            span.record("status", status(value));
            span.record("outcome", "ok");
        }
        Err(error) => {
            span.record("outcome", error_kind(error));
        }
    }
}

fn error_kind(error: &AdapterError) -> &'static str {
    match error {
        AdapterError::Unavailable(_) => "unavailable",
        AdapterError::Credential(_) => "credential",
        AdapterError::Network(network) => match network.kind {
            NetworkErrorKind::Timeout => "network_timeout",
            NetworkErrorKind::Connect => "network_connect",
            NetworkErrorKind::Request => "network_request",
            NetworkErrorKind::ResponseBody => "network_response_body",
        },
        AdapterError::Provider(_) => "provider_error",
        AdapterError::Protocol(_) => "protocol",
        AdapterError::Size(_) => "size",
        AdapterError::File(_) => "file",
        AdapterError::Cancelled => "cancelled",
    }
}
