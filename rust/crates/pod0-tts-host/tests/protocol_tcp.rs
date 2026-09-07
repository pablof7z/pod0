mod support;

use std::time::Duration;

use pod0_tts_host::{
    AudioMediaType, CancellationToken, ElevenLabsEndpoint, GenerationRequest, ProviderSecret,
    TtsClient, TtsLimits,
};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use support::{TcpTestServer, TestDirectory, VALID_MP3, response};

#[tokio::test]
async fn sends_elevenlabs_protocol_and_atomically_returns_raw_evidence() {
    let audio = VALID_MP3.to_vec();
    let response_audio = audio.clone();
    let server = TcpTestServer::spawn(1, move |_index, request| {
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with(
            "POST /v1/text-to-speech/voice-123/stream?output_format=mp3_44100_128 HTTP/1.1\r\n"
        ));
        assert!(lower.contains("\r\nxi-api-key: test-provider-secret\r\n"));
        assert!(lower.contains("\r\naccept: audio/*\r\n"));
        assert!(lower.contains("\r\naccept-encoding: identity\r\n"));
        let body = request.split_once("\r\n\r\n").unwrap().1;
        let json: Value = serde_json::from_str(body).unwrap();
        assert_eq!(json["text"], "Generate this actual request.");
        assert_eq!(json["model_id"], "eleven_multilingual_v2");
        response(
            "200 OK",
            &[
                ("Content-Type", "audio/mpeg; charset=binary"),
                ("request-id", "request-42"),
            ],
            &response_audio,
        )
    });
    let directory = TestDirectory::new("success");
    let staged_path = directory.path().join("episode.mp3");
    let endpoint = ElevenLabsEndpoint::new_loopback_http(&server.url()).unwrap();
    let secret = ProviderSecret::new("test-provider-secret");
    let request = GenerationRequest {
        endpoint: &endpoint,
        secret: &secret,
        model_id: "eleven_multilingual_v2",
        voice_id: "voice-123",
        script: "Generate this actual request.",
        output_format: Some("mp3_44100_128"),
        staged_path: &staged_path,
        timeout: Duration::from_secs(2),
        limits: TtsLimits {
            maximum_script_bytes: 1_024,
            maximum_output_bytes: 4_096,
        },
    };

    let debug = format!("{request:?}");
    assert!(!debug.contains("test-provider-secret"));
    assert!(!debug.contains("Generate this actual request."));
    let evidence = TtsClient::default()
        .generate(&request, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(std::fs::read(&staged_path).unwrap(), audio);
    assert_eq!(evidence.staged_path, staged_path);
    assert_eq!(evidence.media_type, AudioMediaType::Mpeg);
    assert_eq!(evidence.byte_count, audio.len() as u64);
    let expected_digest: [u8; 32] = Sha256::digest(&audio).into();
    assert_eq!(evidence.content_digest, expected_digest);
    assert_eq!(evidence.provider.status, 200);
    assert_eq!(evidence.provider.request_id.as_deref(), Some("request-42"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn a_truncated_response_never_exposes_a_partial_final_file() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        let mut response =
            b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nContent-Length: 100\r\nConnection: close\r\n\r\n"
                .to_vec();
        response.extend_from_slice(b"partial");
        response
    });
    let directory = TestDirectory::new("partial");
    let staged_path = directory.path().join("episode.mp3");
    let endpoint = ElevenLabsEndpoint::new_loopback_http(&server.url()).unwrap();
    let secret = ProviderSecret::new("secret");
    let result = TtsClient::default()
        .generate(
            &GenerationRequest {
                endpoint: &endpoint,
                secret: &secret,
                model_id: "model",
                voice_id: "voice",
                script: "script",
                output_format: None,
                staged_path: &staged_path,
                timeout: Duration::from_secs(2),
                limits: TtsLimits {
                    maximum_script_bytes: 64,
                    maximum_output_bytes: 1_024,
                },
            },
            &CancellationToken::new(),
        )
        .await;

    assert!(result.is_err());
    assert!(!staged_path.exists());
    directory.assert_empty();
}

#[tokio::test]
async fn partial_success_and_invalid_or_mismatched_audio_are_never_published() {
    let server = TcpTestServer::spawn(3, |index, _request| match index {
        0 => response(
            "206 Partial Content",
            &[("Content-Type", "audio/mpeg")],
            VALID_MP3,
        ),
        1 => response(
            "200 OK",
            &[("Content-Type", "audio/mpeg")],
            b"ID3\x04\x00\x00server-generated-audio",
        ),
        _ => response("200 OK", &[("Content-Type", "audio/wav")], b"not a WAV"),
    });
    let directory = TestDirectory::new("response-validation");
    let endpoint = ElevenLabsEndpoint::new_loopback_http(&server.url()).unwrap();
    let secret = ProviderSecret::new("secret");

    for (index, output_format) in [
        (0, Some("mp3_44100_128")),
        (1, Some("mp3_44100_128")),
        (2, Some("mp3_44100_128")),
    ] {
        let path = directory.path().join(format!("{index}.mp3"));
        let error = TtsClient::default()
            .generate(
                &GenerationRequest {
                    endpoint: &endpoint,
                    secret: &secret,
                    model_id: "model",
                    voice_id: "voice",
                    script: "script",
                    output_format,
                    staged_path: &path,
                    timeout: Duration::from_secs(2),
                    limits: TtsLimits {
                        maximum_script_bytes: 64,
                        maximum_output_bytes: 4_096,
                    },
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();

        match index {
            0 => assert!(matches!(
                error,
                pod0_tts_host::TtsError::Provider(detail) if detail.status == 206
            )),
            1 => assert!(matches!(
                error,
                pod0_tts_host::TtsError::Protocol(detail)
                    if detail.kind == pod0_tts_host::ProtocolErrorKind::InvalidAudioPayload
            )),
            _ => assert!(matches!(
                error,
                pod0_tts_host::TtsError::Protocol(detail)
                    if detail.kind == pod0_tts_host::ProtocolErrorKind::MismatchedContentType
            )),
        }
        assert!(!path.exists());
    }
    directory.assert_empty();
}

#[tokio::test]
async fn destination_created_during_download_is_not_replaced() {
    let directory = TestDirectory::new("racing-destination");
    let staged_path = directory.path().join("episode.mp3");
    let competing_path = staged_path.clone();
    let server = TcpTestServer::spawn(1, move |_index, _request| {
        std::fs::write(&competing_path, b"competing publication").unwrap();
        response("200 OK", &[("Content-Type", "audio/mpeg")], VALID_MP3)
    });
    let endpoint = ElevenLabsEndpoint::new_loopback_http(&server.url()).unwrap();
    let secret = ProviderSecret::new("secret");
    let result = TtsClient::default()
        .generate(
            &GenerationRequest {
                endpoint: &endpoint,
                secret: &secret,
                model_id: "model",
                voice_id: "voice",
                script: "script",
                output_format: Some("mp3_44100_128"),
                staged_path: &staged_path,
                timeout: Duration::from_secs(2),
                limits: TtsLimits {
                    maximum_script_bytes: 64,
                    maximum_output_bytes: 4_096,
                },
            },
            &CancellationToken::new(),
        )
        .await;

    assert!(matches!(
        result,
        Err(pod0_tts_host::TtsError::File(detail))
            if detail.kind == pod0_tts_host::FileErrorKind::AlreadyExists
    ));
    assert_eq!(
        std::fs::read(&staged_path).unwrap(),
        b"competing publication"
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
