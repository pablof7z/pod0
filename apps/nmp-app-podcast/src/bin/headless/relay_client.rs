//! Async Nostr relay client used by the headless capability host.
//!
//! Each operation opens its own WebSocket connection per relay (no pooling —
//! simple and correct for the headless test harness). Connections are closed
//! cleanly after EOSE or timeout.
//!
//! ## Publish flow
//!
//! Send `["EVENT", <event_value>]`, then wait for
//! `["OK", "<event_id>", true/false, "<msg>"]` or timeout. The relay is
//! added to `accepted_relays` if the `ok` field is `true`.
//!
//! ## Subscribe flow
//!
//! Send `["REQ", sub_id, filter]`, collect `["EVENT", sub_id, <event>]`
//! messages until `["EOSE", sub_id]` arrives or the timeout elapses, then
//! send `["CLOSE", sub_id]`.

use std::collections::HashSet;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Publish a pre-signed Nostr event JSON to multiple relays concurrently.
///
/// Returns `(accepted_relays, errors)` where `errors` contains
/// `(relay_url, error_message)` for every relay that rejected or timed out.
pub async fn publish_event(
    event_json: &str,
    relay_urls: &[String],
    timeout_dur: Duration,
) -> (Vec<String>, Vec<(String, String)>) {
    let event_value: serde_json::Value = match serde_json::from_str(event_json) {
        Ok(v) => v,
        Err(e) => {
            let errors = relay_urls
                .iter()
                .map(|u| (u.clone(), format!("invalid event_json: {e}")))
                .collect();
            return (vec![], errors);
        }
    };

    let msg_text = serde_json::json!(["EVENT", event_value]).to_string();

    let mut handles = Vec::with_capacity(relay_urls.len());
    for url in relay_urls {
        let url_c = url.clone();
        let msg_c = msg_text.clone();
        handles.push((
            url.clone(),
            tokio::spawn(async move { publish_to_relay(&url_c, &msg_c, timeout_dur).await }),
        ));
    }

    let mut accepted = Vec::new();
    let mut errors = Vec::new();
    for (url, handle) in handles {
        match handle.await {
            Ok(Ok(())) => accepted.push(url),
            Ok(Err(e)) => errors.push((url, e)),
            Err(e) => errors.push((url, format!("task panic: {e}"))),
        }
    }
    (accepted, errors)
}

async fn publish_to_relay(relay_url: &str, event_msg: &str, td: Duration) -> Result<(), String> {
    let (ws, _) = timeout(td, connect_async(relay_url))
        .await
        .map_err(|_| "connect timed out".to_string())?
        .map_err(|e| format!("connect error: {e}"))?;

    let (mut write, mut read) = ws.split();
    write
        .send(Message::Text(event_msg.to_string().into()))
        .await
        .map_err(|e| format!("send error: {e}"))?;

    let result = timeout(td, async {
        while let Some(m) = read.next().await {
            match m {
                Ok(Message::Text(t)) => {
                    if let Ok(arr) = serde_json::from_str::<serde_json::Value>(&t) {
                        if arr[0].as_str() == Some("OK") {
                            let ok = arr[2].as_bool().unwrap_or(false);
                            let msg = arr[3].as_str().unwrap_or("").to_string();
                            return if ok { Ok(()) } else { Err(format!("rejected: {msg}")) };
                        }
                    }
                }
                Ok(Message::Close(_)) => return Err("closed before OK".into()),
                Err(e) => return Err(format!("ws error: {e}")),
                _ => {}
            }
        }
        Err("connection closed without OK".into())
    })
    .await
    .map_err(|_| "timeout waiting for OK".to_string())?;

    let _ = write.send(Message::Close(None)).await;
    result
}

/// Subscribe to a filter on multiple relays; return deduplicated events.
pub async fn subscribe_until_eose(
    sub_id: &str,
    filter: &serde_json::Value,
    relay_urls: &[String],
    timeout_dur: Duration,
) -> Vec<serde_json::Value> {
    let req_msg = serde_json::json!(["REQ", sub_id, filter]).to_string();

    let mut handles = Vec::with_capacity(relay_urls.len());
    for url in relay_urls {
        let url_c = url.clone();
        let req_c = req_msg.clone();
        let sub_c = sub_id.to_string();
        handles.push(tokio::spawn(async move {
            subscribe_on_relay(&url_c, &req_c, &sub_c, timeout_dur).await
        }));
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut all: Vec<serde_json::Value> = Vec::new();
    for handle in handles {
        if let Ok(events) = handle.await {
            for ev in events {
                let id = ev.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if seen.insert(id) {
                    all.push(ev);
                }
            }
        }
    }
    all
}

async fn subscribe_on_relay(
    relay_url: &str,
    req_msg: &str,
    sub_id: &str,
    td: Duration,
) -> Vec<serde_json::Value> {
    let ws = match timeout(td, connect_async(relay_url)).await {
        Ok(Ok((s, _))) => s,
        _ => return vec![],
    };

    let (mut write, mut read) = ws.split();
    if write.send(Message::Text(req_msg.to_string().into())).await.is_err() {
        return vec![];
    }

    let mut events = Vec::new();
    let _ = timeout(td, async {
        while let Some(m) = read.next().await {
            match m {
                Ok(Message::Text(t)) => {
                    let Ok(arr) = serde_json::from_str::<serde_json::Value>(&t) else { continue };
                    match arr[0].as_str() {
                        Some("EVENT") if arr[1].as_str() == Some(sub_id) => {
                            events.push(arr[2].clone());
                        }
                        Some("EOSE") if arr[1].as_str() == Some(sub_id) => break,
                        _ => {}
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
    })
    .await;

    let close_msg = serde_json::json!(["CLOSE", sub_id]).to_string();
    let _ = write.send(Message::Text(close_msg.into())).await;
    let _ = write.send(Message::Close(None)).await;
    events
}
