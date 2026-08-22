use std::io::{Read as _, Write as _};
use std::net::TcpListener;

use pod0_cli::{HostConfig, Shell};

#[test]
#[ignore = "requires Pod0Facade store-bootstrap support not yet committed to pod0-storage/pod0-facade — see .planning/phases/01-headless-host-crates/01-VERIFICATION.md; un-ignore once that lands (as of 2026-08-22)"]
fn search_podcasts_hits_a_real_http_endpoint_and_returns_feed_urls() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert!(
            request.starts_with("GET /search?media=podcast&limit=10&term=acquired"),
            "expected iTunes-style search query, got {request:?}"
        );
        let body = r#"{
            "resultCount": 2,
            "results": [
                {"collectionId": 1530353,"trackName":"Acquired","artistName":"Ben Gilbert","feedUrl":"https://acquired.com/feed.xml","artworkUrl100":"https://art/a.jpg","trackCount":180},
                {"collectionId": 999111,"collectionName":"Acquired Finance","artistName":"Other","trackCount":7}
            ]
        }"#;
        write_response(&mut stream, body);
    });

    // Point the host at the local server instead of itunes.apple.com.
    // SAFETY: this test is single-threaded with respect to the env var; no
    // other test reads or writes POD0_PODCAST_SEARCH_URL.
    unsafe {
        std::env::set_var("POD0_PODCAST_SEARCH_URL", format!("http://{address}/search"));
    }

    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    let mut shell = Shell::new(HostConfig::empty()).unwrap();
    assert!(shell.handle(create_request(&store)).ok);

    let search: pod0_cli::CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "search_podcasts",
        "term": "acquired",
        "limit": 10
    }))
    .unwrap();
    let response = shell.handle(search);
    assert!(response.ok, "{:?}", response.error);
    let value = serde_json::to_value(&response).unwrap();
    let results = value.pointer("/result/results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].pointer("/itunes_id"), Some(&serde_json::json!(1530353)));
    assert_eq!(results[0].pointer("/title"), Some(&serde_json::json!("Acquired")));
    assert_eq!(
        results[0].pointer("/feed_url"),
        Some(&serde_json::json!("https://acquired.com/feed.xml"))
    );
    // The second result has no feedUrl; the field is omitted from the DTO.
    assert!(results[1].get("feed_url").is_none());
    server.join().unwrap();
}

fn create_request(path: &std::path::Path) -> pod0_cli::CliRequest {
    serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "create_store",
        "path": path.to_string_lossy()
    }))
    .unwrap()
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

fn write_response(stream: &mut std::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).unwrap();
}