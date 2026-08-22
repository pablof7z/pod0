mod support;

use std::time::Duration;

use pod0_live_hosts::{
    AdapterError, CancellationToken, DownloadRequest, FileErrorKind, HttpLimits, LiveHosts,
    RequestOptions,
};
use support::{TcpTestServer, TestDirectory, response};

#[tokio::test]
async fn download_is_written_then_atomically_finalized() {
    let server = TcpTestServer::spawn(1, |_index, request| {
        assert!(request.starts_with("GET /audio HTTP/1.1\r\n"));
        response(
            "200 OK",
            &[
                ("Content-Type", "audio/mpeg"),
                ("ETag", "audio-v1"),
                ("Last-Modified", "Sun, 16 Aug 2026 20:00:00 GMT"),
            ],
            b"real-audio-bytes",
        )
    });
    let directory = TestDirectory::new();
    let staged_path = directory.path().join("episode.mp3");
    let result = LiveHosts::default()
        .download(
            DownloadRequest {
                url: server.url("/audio"),
                staged_path: staged_path.clone(),
                accept: Some("audio/*".to_owned()),
                entity_tag: None,
                last_modified: None,
                options: RequestOptions {
                    timeout: Duration::from_secs(2),
                    maximum_redirects: 2,
                    limits: HttpLimits {
                        maximum_body_bytes: 1_024,
                        maximum_metadata_bytes: 4_096,
                    },
                },
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.byte_count, 16);
    assert_eq!(std::fs::read(&staged_path).unwrap(), b"real-audio-bytes");
    assert_eq!(result.evidence.content_type.as_deref(), Some("audio/mpeg"));
    let entries = std::fs::read_dir(directory.path())
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1, "temporary file must be finalized");
}

#[tokio::test]
async fn failed_download_leaves_no_staged_or_temporary_file() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        response("404 Not Found", &[], b"missing")
    });
    let directory = TestDirectory::new();
    let staged_path = directory.path().join("missing.mp3");
    let result = LiveHosts::default()
        .download(
            DownloadRequest {
                url: server.url("/missing"),
                staged_path: staged_path.clone(),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: RequestOptions {
                    timeout: Duration::from_secs(2),
                    maximum_redirects: 0,
                    limits: HttpLimits {
                        maximum_body_bytes: 1_024,
                        maximum_metadata_bytes: 4_096,
                    },
                },
            },
            &CancellationToken::new(),
        )
        .await;
    assert!(result.is_err());
    assert!(!staged_path.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn oversized_download_removes_partial_temporary_file() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
          5\r\nfirst\r\n6\r\nsecond\r\n0\r\n\r\n"
            .to_vec()
    });
    let directory = TestDirectory::new();
    let staged_path = directory.path().join("bounded.bin");
    let result = LiveHosts::default()
        .download(
            DownloadRequest {
                url: server.url("/bounded"),
                staged_path: staged_path.clone(),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: RequestOptions {
                    timeout: Duration::from_secs(2),
                    maximum_redirects: 0,
                    limits: HttpLimits {
                        maximum_body_bytes: 5,
                        maximum_metadata_bytes: 4_096,
                    },
                },
            },
            &CancellationToken::new(),
        )
        .await;
    assert!(result.is_err());
    assert!(!staged_path.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn partial_content_download_is_rejected() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        response(
            "206 Partial Content",
            &[("Content-Range", "bytes 0-3/10")],
            b"part",
        )
    });
    let directory = TestDirectory::new();
    let staged_path = directory.path().join("partial.bin");
    let error = LiveHosts::default()
        .download(
            DownloadRequest {
                url: server.url("/partial"),
                staged_path: staged_path.clone(),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: RequestOptions {
                    timeout: Duration::from_secs(2),
                    maximum_redirects: 0,
                    limits: HttpLimits {
                        maximum_body_bytes: 1_024,
                        maximum_metadata_bytes: 4_096,
                    },
                },
            },
            &CancellationToken::new(),
        )
        .await
        .expect_err("partial downloads are not supported");
    assert!(matches!(error, AdapterError::Protocol(_)));
    assert!(!staged_path.exists());
}

#[tokio::test]
async fn finalization_never_replaces_a_racing_destination() {
    let directory = TestDirectory::new();
    let staged_path = directory.path().join("race.bin");
    let path_for_server = staged_path.clone();
    let server = TcpTestServer::spawn(1, move |_index, _request| {
        std::fs::write(&path_for_server, b"incumbent").unwrap();
        response("200 OK", &[], b"download")
    });
    let error = LiveHosts::default()
        .download(
            DownloadRequest {
                url: server.url("/race"),
                staged_path: staged_path.clone(),
                accept: None,
                entity_tag: None,
                last_modified: None,
                options: RequestOptions {
                    timeout: Duration::from_secs(2),
                    maximum_redirects: 0,
                    limits: HttpLimits {
                        maximum_body_bytes: 1_024,
                        maximum_metadata_bytes: 4_096,
                    },
                },
            },
            &CancellationToken::new(),
        )
        .await
        .expect_err("racing destination must not be replaced");
    assert!(matches!(
        error,
        AdapterError::File(ref file) if file.kind == FileErrorKind::AlreadyExists
    ));
    assert_eq!(std::fs::read(&staged_path).unwrap(), b"incumbent");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
