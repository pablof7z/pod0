use std::io::{Read as _, Write as _};
use std::net::TcpListener;

use pod0_cli::{CliRequest, HostConfig, Shell};

#[test]
fn openai_compatible_turn_uses_live_http_and_returns_the_core_projection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(request.starts_with("POST /v1/chat/completions "));
        assert!(!request.contains("\"tools\""));
        assert!(!request.contains("\"tool_choice\""));
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"Live response","tool_calls":[]}}],"usage":{"prompt_tokens":7,"completion_tokens":2}}"#;
        write_response(&mut stream, body);
    });

    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    let config = HostConfig::openai_compatible(
        format!("http://{address}/v1"),
        Some("integration-secret".to_owned()),
    );
    let mut shell = Shell::new(config).unwrap();
    assert!(shell.handle(create_request(&store)).ok);

    let response = shell.handle(ask_request("Answer over the network"));
    assert!(response.ok, "{:?}", response.error);
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(
        response.pointer("/result/stage"),
        Some(&serde_json::Value::String("completed".to_owned()))
    );
    assert_eq!(
        response.pointer("/result/messages/1/content"),
        Some(&serde_json::Value::String("Live response".to_owned()))
    );
    server.join().unwrap();
}

#[test]
fn unexpected_provider_tool_call_fails_without_capability_execution() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(!request.contains("\"tools\""));
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"","tool_calls":[{"id":"unexpected","type":"function","function":{"name":"create_note","arguments":"{}"}}]}}]}"#;
        write_response(&mut stream, body);
    });

    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    let config = HostConfig::openai_compatible(format!("http://{address}/v1"), None);
    let mut shell = Shell::new(config).unwrap();
    assert!(shell.handle(create_request(&store)).ok);

    let response = shell.handle(ask_request("Try an unavailable tool"));
    assert!(response.ok, "{:?}", response.error);
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(
        response.pointer("/result/stage"),
        Some(&serde_json::Value::String("failed".to_owned()))
    );
    assert!(
        response
            .pointer("/result/safe_failure")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.contains("advertises no tools"))
    );
    server.join().unwrap();
}

#[test]
fn ollama_turn_uses_native_live_http_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(request.starts_with("POST /api/chat "));
        assert!(!request.contains("\"tools\""));
        let body = r#"{"message":{"role":"assistant","content":"Ollama response"},"prompt_eval_count":5,"eval_count":2}"#;
        write_response(&mut stream, body);
    });

    let directory = tempfile::tempdir_in(".").unwrap();
    let store = directory.path().join("pod0.sqlite");
    let mut shell = Shell::new(HostConfig::ollama(address.to_string())).unwrap();
    assert!(shell.handle(create_request(&store)).ok);

    let ask: CliRequest = serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "ask_agent",
        "input": "Answer with Ollama",
        "provider": "ollama",
        "model": "integration-model"
    }))
    .unwrap();
    let response = shell.handle(ask);
    assert!(response.ok, "{:?}", response.error);
    let response = serde_json::to_value(response).unwrap();
    assert_eq!(
        response.pointer("/result/messages/1/content"),
        Some(&serde_json::Value::String("Ollama response".to_owned()))
    );
    server.join().unwrap();
}

fn create_request(path: &std::path::Path) -> CliRequest {
    serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "create_store",
        "path": path.to_string_lossy()
    }))
    .unwrap()
}

fn ask_request(input: &str) -> CliRequest {
    serde_json::from_value(serde_json::json!({
        "v": 1,
        "command": "ask_agent",
        "input": input,
        "provider": "open_ai_compatible",
        "model": "integration-model"
    }))
    .unwrap()
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..count]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .unwrap_or_default();
        if request.len() >= header_end + 4 + content_length {
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
