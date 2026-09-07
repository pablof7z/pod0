mod support;

use std::time::Duration;

use pod0_live_hosts::{
    AdapterError, CancellationToken, HttpGetRequest, HttpLimits, LiveHosts, NetworkErrorKind,
    RequestOptions, SizeSubject,
};
use support::{TcpTestServer, response};

fn options(body_limit: u64, timeout: Duration) -> RequestOptions {
    RequestOptions {
        timeout,
        maximum_redirects: 3,
        limits: HttpLimits {
            maximum_body_bytes: body_limit,
            maximum_metadata_bytes: 4_096,
        },
    }
}

#[tokio::test]
async fn follows_redirect_and_returns_bounded_http_evidence() {
    let server = TcpTestServer::spawn(2, |index, request| {
        if index == 0 {
            assert!(request.starts_with("GET /start HTTP/1.1\r\n"));
            response("302 Found", &[("Location", "/final")], b"")
        } else {
            assert!(request.starts_with("GET /final HTTP/1.1\r\n"));
            assert!(request.contains("if-none-match: old-tag\r\n"));
            response(
                "200 OK",
                &[
                    ("ETag", "new-tag"),
                    ("Last-Modified", "Sun, 16 Aug 2026 20:00:00 GMT"),
                    ("Content-Type", "application/json"),
                ],
                br#"{"live":true}"#,
            )
        }
    });
    let result = LiveHosts::default()
        .http_get(
            HttpGetRequest {
                url: server.url("/start"),
                accept: Some("application/json".to_owned()),
                entity_tag: Some("old-tag".to_owned()),
                last_modified: None,
                options: options(1_024, Duration::from_secs(2)),
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.body, br#"{"live":true}"#);
    assert_eq!(result.evidence.status, 200);
    assert_eq!(result.evidence.redirects.len(), 1);
    assert_eq!(result.evidence.redirects[0].status, 302);
    assert!(result.evidence.final_url.ends_with("/final"));
    assert_eq!(result.evidence.entity_tag.as_deref(), Some("new-tag"));
}

#[tokio::test]
async fn rejects_chunked_body_as_soon_as_bound_is_crossed() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
          4\r\nlive\r\n4\r\ndata\r\n0\r\n\r\n"
            .to_vec()
    });
    let error = LiveHosts::default()
        .http_get(
            HttpGetRequest {
                url: server.url("/large"),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: options(4, Duration::from_secs(2)),
            },
            &CancellationToken::new(),
        )
        .await
        .expect_err("body must exceed limit");
    assert!(matches!(
        error,
        AdapterError::Size(ref size) if size.subject == SizeSubject::ResponseBody
    ));
}

#[tokio::test]
async fn rejects_cumulative_redirect_metadata_before_following() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        response(
            "302 Found",
            &[("Location", "/this-redirect-location-is-long")],
            b"",
        )
    });
    let error = LiveHosts::default()
        .http_get(
            HttpGetRequest {
                url: server.url("/start"),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: RequestOptions {
                    timeout: Duration::from_secs(2),
                    maximum_redirects: 3,
                    limits: HttpLimits {
                        maximum_body_bytes: 1_024,
                        maximum_metadata_bytes: 64,
                    },
                },
            },
            &CancellationToken::new(),
        )
        .await
        .expect_err("redirect evidence must exceed the cumulative limit");
    assert!(matches!(
        error,
        AdapterError::Size(ref size) if size.subject == SizeSubject::Metadata
    ));
    assert_eq!(server.accepted_connections(), 1);
}

#[tokio::test]
async fn pre_cancelled_request_performs_no_network_io() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        panic!("pre-cancelled request reached the network")
    });
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = LiveHosts::default()
        .http_get(
            HttpGetRequest {
                url: server.url("/never"),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: options(10, Duration::from_secs(2)),
            },
            &cancellation,
        )
        .await
        .expect_err("pre-cancelled request must fail");
    assert!(matches!(error, AdapterError::Cancelled));
    assert_eq!(server.accepted_connections(), 0);
}

#[tokio::test]
async fn timeout_and_cancellation_interrupt_live_connections() {
    let timeout_server = TcpTestServer::spawn(1, |_index, _request| {
        std::thread::sleep(Duration::from_millis(150));
        response("200 OK", &[], b"late")
    });
    let timeout = LiveHosts::default()
        .http_get(
            HttpGetRequest {
                url: timeout_server.url("/slow"),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: options(10, Duration::from_millis(30)),
            },
            &CancellationToken::new(),
        )
        .await
        .expect_err("request must time out");
    assert!(matches!(
        timeout,
        AdapterError::Network(ref error) if error.kind == NetworkErrorKind::Timeout
    ));

    let cancellation_server = TcpTestServer::spawn(1, |_index, _request| {
        std::thread::sleep(Duration::from_millis(150));
        response("200 OK", &[], b"late")
    });
    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        canceller.cancel();
    });
    let cancelled = LiveHosts::default()
        .http_get(
            HttpGetRequest {
                url: cancellation_server.url("/cancel"),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: options(10, Duration::from_secs(2)),
            },
            &token,
        )
        .await
        .expect_err("request must be cancelled");
    assert!(matches!(cancelled, AdapterError::Cancelled));
}
