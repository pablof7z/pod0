use std::io::{Read as _, Write as _};
use std::net::TcpListener;

use pod0_cli::{CliRequest, HostConfig, Shell};
use pod0_facade::{Pod0Facade, Projection, ProjectionRequest, ProjectionScope};

const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>Network Feed</title>
<item><title>Network Episode</title><guid>network-episode-1</guid>
<pubDate>Mon, 20 Jul 2026 09:00:00 GMT</pubDate>
<enclosure url="https://media.example/episode.mp3" type="audio/mpeg"/>
</item></channel></rss>"#;

#[test]
#[ignore = "requires Pod0Facade store-bootstrap support not yet committed to pod0-storage/pod0-facade — see .planning/phases/01-headless-host-crates/01-VERIFICATION.md; un-ignore once that lands (as of 2026-08-22)"]
fn network_feed_persists_and_reopens_from_the_authoritative_store() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let count = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..count]).starts_with("GET /feed.xml "));
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/rss+xml\r\nETag: \"network-v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            RSS.len(),
            RSS
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    let mut shell = Shell::new(HostConfig::empty()).unwrap();
    let create: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "create_store",
        "path": store.to_string_lossy()
    }))
    .unwrap();
    assert!(shell.handle(create).ok);

    let subscribe: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "subscribe_feed",
        "feed_url": format!("http://{address}/feed.xml")
    }))
    .unwrap();
    let response = shell.handle(subscribe);
    assert!(response.ok, "{:?}", response.error);
    let response_json = serde_json::to_value(response).unwrap();
    assert_eq!(
        response_json.pointer("/result/operation/stage"),
        Some(&serde_json::Value::String("succeeded".to_owned()))
    );
    server.join().unwrap();
    drop(shell);

    let reopened = Pod0Facade::open(store.to_string_lossy().into_owned()).unwrap();
    let Projection::Library { value } = reopened
        .snapshot(ProjectionRequest {
            scope: ProjectionScope::Library,
            offset: 0,
            max_items: 50,
        })
        .projection
    else {
        panic!("expected library projection");
    };
    assert_eq!(value.podcasts.len(), 1);
    assert_eq!(value.subscriptions.len(), 1);
    assert_eq!(value.episodes.len(), 1);
    assert_eq!(value.podcasts[0].title, "Network Feed");
    assert_eq!(value.episodes[0].publisher_guid, "network-episode-1");
}
