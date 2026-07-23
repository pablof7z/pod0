use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use nostr::{Event, EventBuilder, Keys, Kind, Tag, Timestamp};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

pub(super) struct BoundRelay {
    listener: TcpListener,
    url: String,
    acknowledge: bool,
}

pub(super) struct ScriptedRelay {
    url: String,
    published: mpsc::Receiver<u16>,
    handle: JoinHandle<()>,
}

impl BoundRelay {
    pub(super) fn bind(acknowledge: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        Self {
            listener,
            url,
            acknowledge,
        }
    }

    pub(super) fn url(&self) -> String {
        self.url.clone()
    }

    pub(super) fn start(self, relay_list: Option<Event>) -> ScriptedRelay {
        let (published_sender, published) = mpsc::channel();
        let url = self.url.clone();
        let handle = std::thread::spawn(move || {
            let connection_count = if relay_list.is_some() { 2 } else { 1 };
            let mut handlers = Vec::new();
            while handlers.len() < connection_count {
                let (stream, _) = self.listener.accept().unwrap();
                if !is_websocket_request(&stream) {
                    respond_to_relay_information(stream);
                    continue;
                }
                let relay_list = relay_list.clone();
                let published_sender = published_sender.clone();
                let acknowledge = self.acknowledge;
                handlers.push(std::thread::spawn(move || {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(20)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(20)))
                        .unwrap();
                    let mut socket = tungstenite::accept(stream).unwrap();
                    serve(
                        &mut socket,
                        relay_list.as_ref(),
                        acknowledge,
                        &published_sender,
                    );
                }));
            }
            for handler in handlers {
                handler.join().unwrap();
            }
        });
        ScriptedRelay {
            url,
            published,
            handle,
        }
    }
}

fn is_websocket_request(stream: &TcpStream) -> bool {
    let mut bytes = [0_u8; 2_048];
    let count = stream.peek(&mut bytes).unwrap();
    String::from_utf8_lossy(&bytes[..count])
        .to_ascii_lowercase()
        .contains("upgrade: websocket")
}

fn respond_to_relay_information(mut stream: TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = [0_u8; 4_096];
    let _ = stream.read(&mut request);
    let body = "{}";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/nostr+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

impl ScriptedRelay {
    pub(super) fn url(&self) -> String {
        self.url.clone()
    }

    pub(super) fn published(&self) -> &mpsc::Receiver<u16> {
        &self.published
    }

    pub(super) fn join(self) {
        self.handle.join().unwrap();
    }
}

pub(super) fn relay_list_event(secret: &str, relays: &[String]) -> Event {
    let keys = Keys::parse(secret).unwrap();
    let tags = relays
        .iter()
        .map(|relay| Tag::parse(["r", relay.as_str(), "write"]).unwrap())
        .collect::<Vec<_>>();
    EventBuilder::new(Kind::RelayList, "")
        .tags(tags)
        .custom_created_at(Timestamp::from(1_799_999_999_u64))
        .sign_with_keys(&keys)
        .unwrap()
}

fn serve(
    socket: &mut WebSocket<TcpStream>,
    relay_list: Option<&Event>,
    acknowledge: bool,
    published: &mpsc::Sender<u16>,
) {
    while let Ok(message) = socket.read() {
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
            continue;
        };
        let Some(values) = value.as_array() else {
            continue;
        };
        match values.first().and_then(Value::as_str) {
            Some("REQ") => respond_to_query(socket, values, relay_list),
            Some("EVENT") => respond_to_write(socket, values, acknowledge, published),
            Some("CLOSE") => {}
            _ => {}
        }
    }
}

fn respond_to_query(
    socket: &mut WebSocket<TcpStream>,
    values: &[Value],
    relay_list: Option<&Event>,
) {
    let Some(subscription_id) = values.get(1).and_then(Value::as_str) else {
        return;
    };
    let asks_for_relay_list = values.iter().skip(2).any(|filter| {
        filter
            .get("kinds")
            .and_then(Value::as_array)
            .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_u64() == Some(10_002)))
    });
    if asks_for_relay_list && let Some(event) = relay_list {
        send(socket, json!(["EVENT", subscription_id, event]));
    }
    send(socket, json!(["EOSE", subscription_id]));
}

fn respond_to_write(
    socket: &mut WebSocket<TcpStream>,
    values: &[Value],
    acknowledge: bool,
    published: &mpsc::Sender<u16>,
) {
    let Some(event) = values.get(1) else {
        return;
    };
    let Some(id) = event.get("id").and_then(Value::as_str) else {
        return;
    };
    let Some(kind) = event
        .get("kind")
        .and_then(Value::as_u64)
        .and_then(|kind| u16::try_from(kind).ok())
    else {
        return;
    };
    let _ = published.send(kind);
    let reason = if acknowledge {
        ""
    } else {
        "scripted rejection"
    };
    send(socket, json!(["OK", id, acknowledge, reason]));
}

fn send(socket: &mut WebSocket<TcpStream>, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .unwrap();
}
