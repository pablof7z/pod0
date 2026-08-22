mod support;

use std::time::Duration;

use pod0_live_hosts::{
    AdapterError, CancellationToken, ChatMessage, ChatRequest, ChatRole, HttpLimits, LiveHosts,
    NetworkErrorKind, OpenAiChatRequest, ProviderEndpoint, ToolChoice,
};
use support::TcpTestServer;

#[tokio::test]
async fn error_body_stream_failure_is_propagated() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        b"HTTP/1.1 500 Internal Server Error\r\n\
          Transfer-Encoding: chunked\r\n\
          Connection: close\r\n\r\n\
          4\r\nfail\r\n\
          invalid-chunk\r\n"
            .to_vec()
    });
    let error = LiveHosts::default()
        .openai_chat(
            OpenAiChatRequest {
                endpoint: ProviderEndpoint::unauthenticated(server.url("/chat")),
                chat: ChatRequest {
                    model: "model".to_owned(),
                    messages: vec![ChatMessage::text(ChatRole::User, "hello")],
                    tools: Vec::new(),
                    tool_choice: ToolChoice::None,
                    temperature: None,
                    maximum_completion_tokens: None,
                    timeout: Duration::from_secs(2),
                    limits: HttpLimits {
                        maximum_body_bytes: 1_024,
                        maximum_metadata_bytes: 4_096,
                    },
                    maximum_output_bytes: 1_024,
                },
            },
            &CancellationToken::new(),
        )
        .await
        .expect_err("malformed error body must not be reported as complete");
    assert!(
        matches!(
            &error,
            AdapterError::Network(network) if network.kind == NetworkErrorKind::ResponseBody
        ),
        "{error:?}"
    );
}
