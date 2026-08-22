mod connect;
mod protocol;

use std::{
    future::Future,
    time::{Duration, Instant},
};

use pod0_application::PersistedEffectLeaseIdentity;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, tungstenite::Error as WebSocketError,
};

use crate::{
    AmbiguityCause, CancellationToken, PublisherConfig, RelayFailureCode, RelayFailureStage,
    RelayOutcome, SigningSecret, config::RelayTarget, event::SignedDraft,
    publisher::unix_milliseconds,
};

pub(crate) type RelayWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) struct AttemptResult {
    pub outcome: Option<RelayOutcome>,
    pub stop: Option<StopReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopReason {
    Cancelled,
    LeaseExpired,
    OperationTimedOut,
}

#[derive(Clone, Copy)]
pub(crate) struct Deadline {
    pub at: Instant,
    pub elapsed: StopReason,
}

pub(super) enum ControlError {
    Stop(StopReason),
    Deadline,
}

pub(crate) async fn publish_to_relay(
    target: &RelayTarget,
    signed: &SignedDraft,
    signing_secret: &SigningSecret,
    config: &PublisherConfig,
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
    operation: Deadline,
) -> AttemptResult {
    let websocket =
        match connect::open_websocket(target, config, cancellation, lease, operation).await {
            Ok(websocket) => websocket,
            Err(result) => return result,
        };
    protocol::publish_and_await_ack(
        websocket,
        target,
        signed,
        signing_secret,
        config,
        cancellation,
        lease,
        operation,
    )
    .await
}

pub(super) async fn controlled<F, T>(
    future: F,
    deadline: Instant,
    config: &PublisherConfig,
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
    operation: Deadline,
) -> Result<T, ControlError>
where
    F: Future<Output = T>,
{
    let stop = wait_for_stop(cancellation, lease, operation, config.cancellation_poll_interval);
    tokio::pin!(future);
    tokio::pin!(stop);
    let timer = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
    tokio::pin!(timer);
    tokio::select! {
        biased;
        reason = &mut stop => Err(ControlError::Stop(reason)),
        _ = &mut timer => match stop_reason(cancellation, lease, operation) {
            Some(reason) => Err(ControlError::Stop(reason)),
            None => Err(ControlError::Deadline),
        },
        result = &mut future => match stop_reason(cancellation, lease, operation) {
            Some(reason) => Err(ControlError::Stop(reason)),
            None => Ok(result),
        },
    }
}

async fn wait_for_stop(
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
    operation: Deadline,
    poll_interval: Duration,
) -> StopReason {
    loop {
        if let Some(reason) = stop_reason(cancellation, lease, operation) {
            return reason;
        }
        tokio::time::sleep(poll_interval).await;
    }
}

pub(super) fn acknowledged(target: &RelayTarget, message: String) -> AttemptResult {
    AttemptResult {
        outcome: Some(RelayOutcome::Acknowledged {
            relay_url: target.request_url.clone(),
            route_id: target.route_id,
            observed_at: pod0_domain::UnixTimestampMilliseconds::new(unix_milliseconds()),
            message,
        }),
        stop: None,
    }
}

pub(super) fn rejected(
    target: &RelayTarget,
    code: RelayFailureCode,
    message: String,
) -> AttemptResult {
    AttemptResult {
        outcome: Some(RelayOutcome::Rejected {
            relay_url: target.request_url.clone(),
            route_id: target.route_id,
            observed_at: pod0_domain::UnixTimestampMilliseconds::new(unix_milliseconds()),
            code,
            message,
        }),
        stop: None,
    }
}

pub(super) fn failed(
    target: &RelayTarget,
    stage: RelayFailureStage,
    code: RelayFailureCode,
    detail: impl AsRef<str>,
) -> AttemptResult {
    AttemptResult {
        outcome: Some(RelayOutcome::Failed {
            relay_url: target.request_url.clone(),
            route_id: target.route_id,
            stage,
            code,
            detail: sanitize(detail.as_ref()),
        }),
        stop: None,
    }
}

pub(super) fn ambiguous(
    target: &RelayTarget,
    cause: AmbiguityCause,
    detail: impl AsRef<str>,
    stop: Option<StopReason>,
) -> AttemptResult {
    AttemptResult {
        outcome: Some(RelayOutcome::HandoffAmbiguous {
            relay_url: target.request_url.clone(),
            route_id: target.route_id,
            cause,
            detail: sanitize(detail.as_ref()),
        }),
        stop,
    }
}

pub(super) fn stopped_without_handoff(stop: StopReason) -> AttemptResult {
    AttemptResult {
        outcome: None,
        stop: Some(stop),
    }
}

pub(super) fn stop_reason(
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
    operation: Deadline,
) -> Option<StopReason> {
    if cancellation.is_cancelled() {
        Some(StopReason::Cancelled)
    } else if unix_milliseconds() >= lease.expires_at.value {
        Some(StopReason::LeaseExpired)
    } else if Instant::now() >= operation.at {
        Some(operation.elapsed)
    } else {
        None
    }
}

pub(super) fn ambiguity_for_stop(stop: StopReason) -> AmbiguityCause {
    match stop {
        StopReason::Cancelled => AmbiguityCause::Cancelled,
        StopReason::LeaseExpired => AmbiguityCause::LeaseExpired,
        StopReason::OperationTimedOut => AmbiguityCause::OperationTimedOut,
    }
}

pub(super) fn bounded_deadline(duration: Duration, outer: Instant) -> Instant {
    Instant::now()
        .checked_add(duration)
        .unwrap_or(outer)
        .min(outer)
}

pub(super) fn safe_websocket_detail(error: &WebSocketError) -> String {
    match error {
        WebSocketError::Io(error) => format!("relay I/O error: {}", error.kind()),
        WebSocketError::Tls(_) => "relay TLS failure".into(),
        WebSocketError::Http(response) => {
            format!("relay HTTP status {}", response.status())
        }
        WebSocketError::Capacity(_) => "relay message exceeded configured bounds".into(),
        WebSocketError::Protocol(_) => "relay WebSocket protocol failure".into(),
        WebSocketError::Utf8(_) => "relay returned invalid UTF-8".into(),
        WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed => {
            "relay connection closed".into()
        }
        _ => "relay WebSocket failure".into(),
    }
}

pub(super) fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(512)
        .collect()
}
