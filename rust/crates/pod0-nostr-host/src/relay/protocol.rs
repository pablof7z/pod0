use std::time::Instant;

use futures_util::{SinkExt as _, StreamExt as _};
use nostr::{ClientMessage, EventBuilder, JsonUtil as _, RelayMessage};
use pod0_application::PersistedEffectLeaseIdentity;
use tokio_tungstenite::tungstenite::Message;

use crate::{
    AmbiguityCause, CancellationToken, PublisherConfig, RelayFailureCode, SigningSecret,
    config::RelayTarget, event::SignedDraft,
};

use super::{
    AttemptResult, ControlError, Deadline, RelayWebSocket, acknowledged, ambiguity_for_stop,
    ambiguous, bounded_deadline, controlled, rejected, safe_websocket_detail, sanitize,
    stop_reason,
};

#[allow(clippy::too_many_arguments)]
pub(super) async fn publish_and_await_ack(
    mut websocket: RelayWebSocket,
    target: &RelayTarget,
    signed: &SignedDraft,
    signing_secret: &SigningSecret,
    config: &PublisherConfig,
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
    operation: Deadline,
) -> AttemptResult {
    let acknowledgement_deadline = bounded_deadline(config.acknowledgement_timeout, operation.at);
    if let Err(result) = send(
        &mut websocket,
        Message::text(signed.wire_message.clone()),
        target,
        AmbiguityCause::SendFailed,
        acknowledgement_deadline,
        config,
        cancellation,
        lease,
        operation,
    )
    .await
    {
        return result;
    }

    let mut auth_id = None;
    let mut auth_challenges = 0_usize;
    let mut auth_required_rejection = None;
    loop {
        let frame = match controlled(
            websocket.next(),
            acknowledgement_deadline,
            config,
            cancellation,
            lease,
            operation,
        )
        .await
        {
            Ok(Some(Ok(frame))) => frame,
            Ok(Some(Err(error))) => {
                return ambiguous(
                    target,
                    AmbiguityCause::Disconnected,
                    safe_websocket_detail(&error),
                    None,
                );
            }
            Ok(None) => {
                return ambiguous(
                    target,
                    AmbiguityCause::Disconnected,
                    "relay disconnected before acknowledging the event",
                    None,
                );
            }
            Err(ControlError::Stop(stop)) => {
                return ambiguous(
                    target,
                    ambiguity_for_stop(stop),
                    "publication stopped after event handoff",
                    Some(stop),
                );
            }
            Err(ControlError::Deadline) => {
                if let Some(message) = auth_required_rejection {
                    return rejected(target, RelayFailureCode::AuthenticationRequired, message);
                }
                return ambiguous(
                    target,
                    AmbiguityCause::AcknowledgementTimedOut,
                    "relay did not acknowledge before the configured deadline",
                    None,
                );
            }
        };
        if matches!(frame, Message::Ping(_) | Message::Pong(_)) {
            continue;
        }
        if matches!(frame, Message::Close(_)) {
            return ambiguous(
                target,
                AmbiguityCause::Disconnected,
                "relay closed before acknowledging the event",
                None,
            );
        }
        let text = match frame.to_text() {
            Ok(text) => text,
            Err(_) => {
                return ambiguous(
                    target,
                    AmbiguityCause::ProtocolFailure,
                    "relay returned a non-UTF-8 message",
                    None,
                );
            }
        };
        let relay_message = match RelayMessage::from_json(text) {
            Ok(message) => message,
            Err(_) => {
                return ambiguous(
                    target,
                    AmbiguityCause::ProtocolFailure,
                    "relay returned an invalid Nostr message",
                    None,
                );
            }
        };
        match relay_message {
            RelayMessage::Ok {
                event_id,
                status,
                message,
            } if event_id == signed.event.id => {
                let message = sanitize(&message);
                if status {
                    let mut result = acknowledged(target, message);
                    result.stop = stop_reason(cancellation, lease, operation);
                    return result;
                }
                if is_auth_required(&message) {
                    auth_required_rejection = Some(message);
                    continue;
                }
                return rejected(target, RelayFailureCode::RelayRejected, message);
            }
            RelayMessage::Ok {
                event_id,
                status,
                message,
            } if Some(event_id) == auth_id => {
                auth_id = None;
                if !status {
                    return rejected(
                        target,
                        RelayFailureCode::AuthenticationRejected,
                        sanitize(&message),
                    );
                }
                if let Err(result) = send(
                    &mut websocket,
                    Message::text(signed.wire_message.clone()),
                    target,
                    AmbiguityCause::AuthenticationFailed,
                    acknowledgement_deadline,
                    config,
                    cancellation,
                    lease,
                    operation,
                )
                .await
                {
                    return result;
                }
                auth_required_rejection = None;
            }
            RelayMessage::Auth { challenge } => {
                if auth_challenges >= config.maximum_authentication_challenges {
                    return ambiguous(
                        target,
                        AmbiguityCause::AuthenticationFailed,
                        "relay exceeded the authentication challenge limit",
                        None,
                    );
                }
                auth_challenges += 1;
                if let Some(stop) = stop_reason(cancellation, lease, operation) {
                    return ambiguous(
                        target,
                        ambiguity_for_stop(stop),
                        "publication stopped before relay authentication",
                        Some(stop),
                    );
                }
                if Instant::now() >= acknowledgement_deadline {
                    return ambiguous(
                        target,
                        AmbiguityCause::AcknowledgementTimedOut,
                        "relay authentication exceeded the acknowledgement deadline",
                        None,
                    );
                }
                let unsigned =
                    EventBuilder::auth(challenge.into_owned(), target.url.clone())
                        .build(signing_secret.public_key());
                let auth_event = match signing_secret.sign_event(unsigned) {
                    Ok(event) => event,
                    Err(()) => {
                        return ambiguous(
                            target,
                            AmbiguityCause::AuthenticationFailed,
                            "relay authentication signing failed",
                            None,
                        );
                    }
                };
                auth_id = Some(auth_event.id);
                let wire = match ClientMessage::auth(auth_event).try_as_json() {
                    Ok(wire) => wire,
                    Err(_) => {
                        return ambiguous(
                            target,
                            AmbiguityCause::AuthenticationFailed,
                            "relay authentication serialization failed",
                            None,
                        );
                    }
                };
                if let Err(result) = send(
                    &mut websocket,
                    Message::text(wire),
                    target,
                    AmbiguityCause::AuthenticationFailed,
                    acknowledgement_deadline,
                    config,
                    cancellation,
                    lease,
                    operation,
                )
                .await
                {
                    return result;
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn send(
    websocket: &mut RelayWebSocket,
    message: Message,
    target: &RelayTarget,
    failure_cause: AmbiguityCause,
    deadline: Instant,
    config: &PublisherConfig,
    cancellation: &CancellationToken,
    lease: PersistedEffectLeaseIdentity,
    operation: Deadline,
) -> Result<(), AttemptResult> {
    match controlled(
        websocket.send(message),
        deadline,
        config,
        cancellation,
        lease,
        operation,
    )
    .await
    {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(ambiguous(
            target,
            failure_cause,
            safe_websocket_detail(&error),
            None,
        )),
        Err(ControlError::Stop(stop)) => Err(ambiguous(
            target,
            ambiguity_for_stop(stop),
            "publication stopped during a relay write",
            Some(stop),
        )),
        Err(ControlError::Deadline) => Err(ambiguous(
            target,
            AmbiguityCause::AcknowledgementTimedOut,
            "relay write exceeded the acknowledgement deadline",
            None,
        )),
    }
}

fn is_auth_required(message: &str) -> bool {
    message
        .split_once(':')
        .is_some_and(|(prefix, _)| prefix.eq_ignore_ascii_case("auth-required"))
}
