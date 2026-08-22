use std::{
    io,
    net::{IpAddr, SocketAddr},
    time::Instant,
};

use pod0_application::PersistedEffectLeaseIdentity;
use tokio::net::{TcpStream, lookup_host};
use tokio_tungstenite::{
    client_async_tls_with_config,
    tungstenite::{Error as WebSocketError, protocol::WebSocketConfig},
};

use crate::{
    CancellationToken, PublisherConfig, RelayFailureCode, RelayFailureStage, config::RelayTarget,
};

use super::{
    AttemptResult, ControlError, Deadline, RelayWebSocket, bounded_deadline, controlled, failed,
    stopped_without_handoff,
};

pub(super) async fn open_websocket(
    target: &RelayTarget,
    config: &PublisherConfig,
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
    operation: Deadline,
) -> Result<RelayWebSocket, AttemptResult> {
    let deadline = bounded_deadline(config.connect_timeout, operation.at);
    let addresses = resolve(target, deadline, operation, config, cancellation, lease).await?;
    let stream = connect(
        target,
        &addresses,
        deadline,
        operation,
        config,
        cancellation,
        lease,
    )
    .await?;
    let websocket_config = WebSocketConfig::default()
        .max_message_size(Some(config.maximum_incoming_message_bytes))
        .max_frame_size(Some(config.maximum_incoming_message_bytes));
    match controlled(
        client_async_tls_with_config(
            target.request_url.as_str(),
            stream,
            Some(websocket_config),
            None,
        ),
        deadline,
        config,
        cancellation,
        lease,
        operation,
    )
    .await
    {
        Ok(Ok((websocket, _))) => Ok(websocket),
        Ok(Err(error)) => Err(failed(
            target,
            RelayFailureStage::Handshake,
            handshake_code(&error),
            handshake_detail(&error),
        )),
        Err(ControlError::Stop(stop)) => Err(stopped_without_handoff(stop)),
        Err(ControlError::Deadline) => Err(failed(
            target,
            RelayFailureStage::Handshake,
            RelayFailureCode::Timeout,
            "relay TLS or WebSocket handshake timed out",
        )),
    }
}

async fn resolve(
    target: &RelayTarget,
    deadline: Instant,
    operation: Deadline,
    config: &PublisherConfig,
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
) -> Result<Vec<SocketAddr>, AttemptResult> {
    if let Ok(address) = target.host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(address, target.port)]);
    }
    match controlled(
        lookup_host((target.host.as_str(), target.port)),
        deadline,
        config,
        cancellation,
        lease,
        operation,
    )
    .await
    {
        Ok(Ok(addresses)) => {
            let addresses = addresses.collect::<Vec<_>>();
            if addresses.is_empty() {
                Err(failed(
                    target,
                    RelayFailureStage::Resolve,
                    RelayFailureCode::Dns,
                    "relay DNS resolution returned no addresses",
                ))
            } else {
                Ok(addresses)
            }
        }
        Ok(Err(_)) => Err(failed(
            target,
            RelayFailureStage::Resolve,
            RelayFailureCode::Dns,
            "relay DNS resolution failed",
        )),
        Err(ControlError::Stop(stop)) => Err(stopped_without_handoff(stop)),
        Err(ControlError::Deadline) => Err(failed(
            target,
            RelayFailureStage::Resolve,
            RelayFailureCode::Timeout,
            "relay DNS resolution timed out",
        )),
    }
}

#[allow(clippy::too_many_arguments)]
async fn connect(
    target: &RelayTarget,
    addresses: &[SocketAddr],
    deadline: Instant,
    operation: Deadline,
    config: &PublisherConfig,
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
) -> Result<TcpStream, AttemptResult> {
    let mut last_error = None;
    for address in addresses {
        match controlled(
            TcpStream::connect(*address),
            deadline,
            config,
            cancellation,
            lease,
            operation,
        )
        .await
        {
            Ok(Ok(stream)) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Ok(Err(error)) => last_error = Some(error),
            Err(ControlError::Stop(stop)) => return Err(stopped_without_handoff(stop)),
            Err(ControlError::Deadline) => {
                return Err(failed(
                    target,
                    RelayFailureStage::Connect,
                    RelayFailureCode::Timeout,
                    "relay connection timed out",
                ));
            }
        }
    }
    let (code, detail) = match last_error {
        Some(error) if error.kind() == io::ErrorKind::ConnectionRefused => (
            RelayFailureCode::ConnectionRefused,
            "relay refused the connection",
        ),
        Some(error) if error.kind() == io::ErrorKind::TimedOut => {
            (RelayFailureCode::Timeout, "relay connection timed out")
        }
        _ => (
            RelayFailureCode::WebSocket,
            "relay connection could not be established",
        ),
    };
    Err(failed(target, RelayFailureStage::Connect, code, detail))
}

fn handshake_code(error: &WebSocketError) -> RelayFailureCode {
    match error {
        WebSocketError::Tls(_) => RelayFailureCode::Tls,
        WebSocketError::Http(_) => RelayFailureCode::Http,
        WebSocketError::Io(error) if error.kind() == io::ErrorKind::TimedOut => {
            RelayFailureCode::Timeout
        }
        _ => RelayFailureCode::WebSocket,
    }
}

fn handshake_detail(error: &WebSocketError) -> &'static str {
    match error {
        WebSocketError::Tls(_) => "relay TLS handshake failed",
        WebSocketError::Http(_) => "relay rejected the WebSocket handshake",
        WebSocketError::Io(error) if error.kind() == io::ErrorKind::TimedOut => {
            "relay TLS or WebSocket handshake timed out"
        }
        _ => "relay TLS or WebSocket handshake failed",
    }
}
