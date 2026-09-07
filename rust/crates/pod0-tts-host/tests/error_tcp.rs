mod support;

use std::{fs, thread, time::Duration};

use pod0_tts_host::{
    CancellationToken, ElevenLabsEndpoint, FileErrorKind, GenerationRequest, LimitSubject,
    ProtocolErrorKind, ProviderSecret, TtsClient, TtsError, TtsLimits,
};
use support::{TcpTestServer, TestDirectory, chunked_response, response};

fn limits(script: usize, output: u64) -> TtsLimits {
    TtsLimits {
        maximum_script_bytes: script,
        maximum_output_bytes: output,
    }
}

#[tokio::test]
async fn provider_errors_are_typed_bounded_and_do_not_echo_secrets() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        response(
            "401 Unauthorized",
            &[("Content-Type", "application/json")],
            b"{\"echo\":\"provider-secret\",\"detail\":\"denied\"}",
        )
    });
    let directory = TestDirectory::new("provider-error");
    let path = directory.path().join("audio.mp3");
    let endpoint = ElevenLabsEndpoint::new_loopback_http(&server.url()).unwrap();
    let secret = ProviderSecret::new("provider-secret");
    let error = TtsClient::default()
        .generate(
            &request(&endpoint, &secret, "script", &path, limits(64, 64)),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        TtsError::Provider(ref detail)
            if detail.status == 401 && detail.response_body_bytes > 0 && !detail.body_truncated
    ));
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains("provider-secret"));
    assert!(!rendered.contains("denied"));
    directory.assert_empty();
}

#[tokio::test]
async fn script_and_streaming_output_bounds_are_strict() {
    let directory = TestDirectory::new("bounds");
    let path = directory.path().join("audio.mp3");
    let endpoint = ElevenLabsEndpoint::new_loopback_http("http://127.0.0.1:9").unwrap();
    let secret = ProviderSecret::new("secret");
    let script_error = TtsClient::default()
        .generate(
            &request(&endpoint, &secret, "12345", &path, limits(4, 64)),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        script_error,
        TtsError::Limit(detail)
            if detail.subject == LimitSubject::Script && detail.observed == Some(5)
    ));
    directory.assert_empty();

    let server = TcpTestServer::spawn(1, |_index, _request| {
        chunked_response(
            "200 OK",
            &[("Content-Type", "audio/mpeg")],
            &[b"1234", b"5678"],
        )
    });
    let endpoint = ElevenLabsEndpoint::new_loopback_http(&server.url()).unwrap();
    let output_error = TtsClient::default()
        .generate(
            &request(&endpoint, &secret, "script", &path, limits(64, 6)),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        output_error,
        TtsError::Limit(detail)
            if detail.subject == LimitSubject::Output && detail.observed == Some(8)
    ));
    directory.assert_empty();
}

#[tokio::test]
async fn content_type_and_empty_audio_are_protocol_errors() {
    let server = TcpTestServer::spawn(3, |index, _request| match index {
        0 => response("200 OK", &[], b"audio"),
        1 => response("200 OK", &[("Content-Type", "application/json")], b"audio"),
        _ => response("200 OK", &[("Content-Type", "audio/mpeg")], b""),
    });
    let directory = TestDirectory::new("protocol");
    let endpoint = ElevenLabsEndpoint::new_loopback_http(&server.url()).unwrap();
    let secret = ProviderSecret::new("secret");
    for (index, expected) in [
        (0, ProtocolErrorKind::MissingContentType),
        (1, ProtocolErrorKind::UnsupportedContentType),
        (2, ProtocolErrorKind::EmptyAudio),
    ] {
        let path = directory.path().join(format!("{index}.mp3"));
        let error = TtsClient::default()
            .generate(
                &request(&endpoint, &secret, "script", &path, limits(64, 64)),
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            TtsError::Protocol(detail) if detail.kind == expected
        ));
    }
    directory.assert_empty();
}

#[tokio::test]
async fn timeout_and_cancellation_interrupt_live_connections_and_clean_files() {
    let timeout_server = TcpTestServer::spawn(1, |_index, _request| {
        thread::sleep(Duration::from_millis(150));
        response("200 OK", &[("Content-Type", "audio/mpeg")], b"late")
    });
    let directory = TestDirectory::new("controls");
    let timeout_path = directory.path().join("timeout.mp3");
    let timeout_endpoint = ElevenLabsEndpoint::new_loopback_http(&timeout_server.url()).unwrap();
    let secret = ProviderSecret::new("secret");
    let timeout = TtsClient::default()
        .generate(
            &GenerationRequest {
                timeout: Duration::from_millis(20),
                ..request(
                    &timeout_endpoint,
                    &secret,
                    "script",
                    &timeout_path,
                    limits(64, 64),
                )
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(timeout, TtsError::Timeout));

    let cancel_server = TcpTestServer::spawn(1, |_index, _request| {
        thread::sleep(Duration::from_millis(150));
        response("200 OK", &[("Content-Type", "audio/mpeg")], b"late")
    });
    let cancel_path = directory.path().join("cancel.mp3");
    let cancel_endpoint = ElevenLabsEndpoint::new_loopback_http(&cancel_server.url()).unwrap();
    let cancellation = CancellationToken::new();
    let canceller = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        canceller.cancel();
    });
    let cancelled = TtsClient::default()
        .generate(
            &request(
                &cancel_endpoint,
                &secret,
                "script",
                &cancel_path,
                limits(64, 64),
            ),
            &cancellation,
        )
        .await
        .unwrap_err();
    assert!(matches!(cancelled, TtsError::Cancelled));
    directory.assert_empty();
}

#[tokio::test]
async fn existing_output_is_never_replaced_or_networked() {
    let directory = TestDirectory::new("existing");
    let path = directory.path().join("audio.mp3");
    fs::write(&path, b"keep").unwrap();
    let endpoint = ElevenLabsEndpoint::new_loopback_http("http://127.0.0.1:9").unwrap();
    let secret = ProviderSecret::new("secret");
    let error = TtsClient::default()
        .generate(
            &request(&endpoint, &secret, "script", &path, limits(64, 64)),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        TtsError::File(detail) if detail.kind == FileErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(path).unwrap(), b"keep");
}

#[tokio::test]
async fn pre_cancelled_request_never_creates_or_publishes_a_file() {
    let directory = TestDirectory::new("pre-cancelled");
    let path = directory.path().join("audio.mp3");
    let endpoint = ElevenLabsEndpoint::new_loopback_http("http://127.0.0.1:9").unwrap();
    let secret = ProviderSecret::new("secret");
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let error = TtsClient::default()
        .generate(
            &request(&endpoint, &secret, "script", &path, limits(64, 64)),
            &cancellation,
        )
        .await
        .unwrap_err();

    assert!(matches!(error, TtsError::Cancelled));
    directory.assert_empty();
}

#[tokio::test]
async fn unsupported_requested_format_is_rejected_before_file_or_network_work() {
    let directory = TestDirectory::new("unsupported-format");
    let path = directory.path().join("audio.bin");
    let endpoint = ElevenLabsEndpoint::new_loopback_http("http://127.0.0.1:9").unwrap();
    let secret = ProviderSecret::new("secret");
    let error = TtsClient::default()
        .generate(
            &GenerationRequest {
                output_format: Some("archive_44100"),
                ..request(&endpoint, &secret, "script", &path, limits(64, 64))
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        TtsError::InvalidRequest(detail)
            if detail.field == pod0_tts_host::InvalidRequestField::OutputFormat
    ));
    directory.assert_empty();
}

fn request<'a>(
    endpoint: &'a ElevenLabsEndpoint,
    secret: &'a ProviderSecret,
    script: &'a str,
    path: &'a std::path::Path,
    limits: TtsLimits,
) -> GenerationRequest<'a> {
    GenerationRequest {
        endpoint,
        secret,
        model_id: "model",
        voice_id: "voice",
        script,
        output_format: None,
        staged_path: path,
        timeout: Duration::from_secs(2),
        limits,
    }
}
