use std::io::{Read as _, Write as _};
use std::net::TcpListener;

use pod0_cli::{HostConfig, HostRequestEnvelope, execute_host_request};
use pod0_facade::{
    CancellationId, CommandId, EvidenceGenerationId, EvidenceSpanId, HostRequest, HostRequestId,
    RecallEmbeddingInput, RecallEmbeddingProvider, RecallQueryId, RecallRerankDocument,
    RecallRerankProvider, StateRevision, UnixTimestampMilliseconds,
};

#[test]
fn openai_compatible_embedding_uses_live_http_and_returns_quantized_vector() {
    let server = expect_request("POST /v1/embeddings ", OPENAI_EMBEDDING_BODY);
    let address = server.address();
    let config = HostConfig::openai_compatible(format!("http://{address}/v1"), None);
    let envelope = embed_query_envelope(
        RecallEmbeddingProvider::OpenRouter,
        "openai/text-embedding-3-large",
        "hello",
        4,
    );
    let expected_query_id = match &envelope.request {
        HostRequest::EmbedRecallQuery { query_id, .. } => *query_id,
        _ => panic!("envelope must carry an embedding query"),
    };
    let observation = execute_host_request(config, envelope).unwrap().unwrap();
    server.assert_served();

    let pod0_facade::HostObservation::RecallQueryEmbedded {
        embedding,
        query_id: observed,
    } = observation
    else {
        panic!("observation must be RecallQueryEmbedded, got {observation:?}");
    };
    assert_eq!(observed, expected_query_id);
    assert_eq!(
        embedding.values,
        vec![100_000, 200_000, -300_000, 500_000],
        "provider floats must be quantized to signed millionths"
    );
}

#[test]
fn ollama_span_embeddings_use_live_http_and_preserve_span_order() {
    let body =
        r#"{"embeddings":[[0.1,0.2,0.0],[-0.4,0.5,0.1]],"prompt_eval_count":12,"model":"nomic"}"#;
    let server = expect_request("POST /api/embeddings ", body);
    let address = server.address();
    let config = HostConfig::ollama(address.to_string());
    let spans = vec![
        RecallEmbeddingInput {
            span_id: EvidenceSpanId::from_parts(0, 1),
            text: "first span".to_owned(),
        },
        RecallEmbeddingInput {
            span_id: EvidenceSpanId::from_parts(0, 2),
            text: "second span".to_owned(),
        },
    ];
    let envelope = embed_spans_envelope(
        RecallEmbeddingProvider::Ollama,
        "nomic-embed-text",
        spans,
        3,
    );
    let observation = execute_host_request(config, envelope).unwrap().unwrap();
    server.assert_served();

    let pod0_facade::HostObservation::RecallSpansEmbedded { embeddings, .. } = observation else {
        panic!("observation must be RecallSpansEmbedded, got {observation:?}");
    };
    assert_eq!(embeddings.len(), 2);
    assert_eq!(embeddings[0].span_id, EvidenceSpanId::from_parts(0, 1));
    assert_eq!(embeddings[0].embedding.values, vec![100_000, 200_000, 0]);
    assert_eq!(embeddings[1].span_id, EvidenceSpanId::from_parts(0, 2));
    assert_eq!(
        embeddings[1].embedding.values,
        vec![-400_000, 500_000, 100_000]
    );
}

#[test]
fn openrouter_rerank_uses_live_http_and_maps_positions_to_ranks() {
    let body = r#"{"results":[{"index":1,"relevance_score":0.91},{"index":0,"relevance_score":0.42}],"id":"rerank-1"}"#;
    let server = expect_request("POST /v1/rerank ", body);
    let address = server.address();
    let config = HostConfig::openai_compatible(format!("http://{address}/v1"), None);
    let candidates = vec![
        RecallRerankDocument {
            span_id: EvidenceSpanId::from_parts(0, 10),
            excerpt: "candidate zero".to_owned(),
        },
        RecallRerankDocument {
            span_id: EvidenceSpanId::from_parts(0, 11),
            excerpt: "candidate one".to_owned(),
        },
    ];
    let envelope = rerank_envelope(
        RecallRerankProvider::OpenRouter,
        "cohere/rerank-v3.5",
        candidates,
    );
    let observation = execute_host_request(config, envelope).unwrap().unwrap();
    server.assert_served();

    let pod0_facade::HostObservation::RecallCandidatesReranked { rankings, .. } = observation
    else {
        panic!("observation must be RecallCandidatesReranked, got {observation:?}");
    };
    assert_eq!(rankings.len(), 2);
    // Relevance order: index 1 first (rank 1), index 0 second (rank 2).
    assert_eq!(rankings[0].span_id, EvidenceSpanId::from_parts(0, 11));
    assert_eq!(rankings[0].rank, 1);
    assert_eq!(rankings[1].span_id, EvidenceSpanId::from_parts(0, 10));
    assert_eq!(rankings[1].rank, 2);
}

const OPENAI_EMBEDDING_BODY: &str = r#"{"data":[{"index":0,"embedding":[0.1,0.2,-0.3,0.5]}],"usage":{"prompt_tokens":3,"total_tokens":4}}"#;

fn embed_query_envelope(
    provider: RecallEmbeddingProvider,
    model: &str,
    text: &str,
    maximum_dimensions: u16,
) -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(1, 1),
        command_id: CommandId::from_parts(1, 2),
        cancellation_id: CancellationId::from_parts(1, 3),
        issued_revision: StateRevision::new(1),
        deadline_at: None,
        request: HostRequest::EmbedRecallQuery {
            query_id: RecallQueryId::from_parts(7, 7),
            provider,
            model: model.to_owned(),
            text: text.to_owned(),
            maximum_dimensions,
        },
    }
}

fn embed_spans_envelope(
    provider: RecallEmbeddingProvider,
    model: &str,
    spans: Vec<RecallEmbeddingInput>,
    maximum_dimensions: u16,
) -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(1, 4),
        command_id: CommandId::from_parts(1, 5),
        cancellation_id: CancellationId::from_parts(1, 6),
        issued_revision: StateRevision::new(2),
        deadline_at: None,
        request: HostRequest::EmbedRecallSpans {
            episode_id: pod0_facade::EpisodeId::from_parts(2, 3),
            generation_id: EvidenceGenerationId::from_parts(4, 5),
            provider,
            model: model.to_owned(),
            spans,
            maximum_dimensions,
        },
    }
}

fn rerank_envelope(
    provider: RecallRerankProvider,
    model: &str,
    candidates: Vec<RecallRerankDocument>,
) -> HostRequestEnvelope {
    HostRequestEnvelope {
        request_id: HostRequestId::from_parts(1, 7),
        command_id: CommandId::from_parts(1, 8),
        cancellation_id: CancellationId::from_parts(1, 9),
        issued_revision: StateRevision::new(3),
        deadline_at: Some(UnixTimestampMilliseconds::new(0)),
        request: HostRequest::RerankRecallCandidates {
            query_id: RecallQueryId::from_parts(9, 9),
            provider,
            model: model.to_owned(),
            query: "podcasts about rust".to_owned(),
            candidates,
        },
    }
}

struct RequestServer {
    address: std::net::SocketAddr,
    handle: Option<std::thread::JoinHandle<()>>,
    served: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl RequestServer {
    fn address(&self) -> std::net::SocketAddr {
        self.address
    }
    fn assert_served(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("server thread panicked");
        }
        assert!(
            self.served.load(std::sync::atomic::Ordering::SeqCst),
            "the headless host never reached the configured endpoint"
        );
    }
}

impl Drop for RequestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn expect_request(expected_request_line: &str, body: &str) -> RequestServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let expected_request_line = expected_request_line.to_owned();
    let body = body.to_owned();
    let served = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let served_for_thread = std::sync::Arc::clone(&served);
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_request(&mut stream);
        assert!(
            request.starts_with(&expected_request_line),
            "expected {expected_request_line:?}, got {request:?}"
        );
        write_response(&mut stream, &body);
        served_for_thread.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    RequestServer {
        address,
        handle: Some(handle),
        served,
    }
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
