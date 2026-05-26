//! Shared async Nostr relay client used by both the library (social handler)
//! and the headless scenario binary.
//!
//! Each call opens its own WebSocket connection per relay (no pooling — simple
//! and correct for request/response style fetches). Connections are closed
//! cleanly after EOSE or timeout.
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

/// Subscribe to a filter on multiple relays; return deduplicated events.
///
/// Events are deduplicated by their `id` field. If the same event is returned
/// by multiple relays, only the first copy is included.
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
                    let Ok(arr) = serde_json::from_str::<serde_json::Value>(&t) else {
                        continue;
                    };
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
