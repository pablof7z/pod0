mod support;

use std::time::Duration;

use pod0_tts_host::{
    CancellationToken, ElevenLabsEndpoint, GenerationRequest, ProtocolErrorKind, ProviderSecret,
    TtsClient, TtsError, TtsLimits,
};
use support::{TcpTestServer, TestDirectory, response};

const VALID_OPUS: &[u8] = include_bytes!("fixtures/valid.opus");
const HEADER_ONLY_OPUS: &[u8] = include_bytes!("fixtures/header-only.opus");
const INCOMPLETE_HEAD_OPUS: &[u8] = include_bytes!("fixtures/incomplete-head.opus");
const INVALID_AUDIO_PACKET_OPUS: &[u8] = include_bytes!("fixtures/invalid-audio-packet.opus");

#[tokio::test]
async fn only_complete_opus_with_a_structural_audio_packet_is_published() {
    let fixtures = [
        VALID_OPUS,
        HEADER_ONLY_OPUS,
        INCOMPLETE_HEAD_OPUS,
        INVALID_AUDIO_PACKET_OPUS,
    ];
    let server = TcpTestServer::spawn(fixtures.len(), move |index, _request| {
        response("200 OK", &[("Content-Type", "audio/ogg")], fixtures[index])
    });
    let directory = TestDirectory::new("opus-validation");
    let endpoint = ElevenLabsEndpoint::new_loopback_http(&server.url()).unwrap();
    let secret = ProviderSecret::new("secret");

    for (index, fixture) in fixtures.into_iter().enumerate() {
        let path = directory.path().join(format!("{index}.opus"));
        let result = TtsClient::default()
            .generate(
                &GenerationRequest {
                    endpoint: &endpoint,
                    secret: &secret,
                    model_id: "model",
                    voice_id: "voice",
                    script: "script",
                    output_format: Some("opus_48000_64"),
                    staged_path: &path,
                    timeout: Duration::from_secs(2),
                    limits: TtsLimits {
                        maximum_script_bytes: 64,
                        maximum_output_bytes: 64 * 1_024,
                    },
                },
                &CancellationToken::new(),
            )
            .await;

        if index == 0 {
            result.unwrap();
            assert_eq!(std::fs::read(path).unwrap(), fixture);
        } else {
            assert!(matches!(
                result,
                Err(TtsError::Protocol(detail))
                    if detail.kind == ProtocolErrorKind::InvalidAudioPayload
            ));
            assert!(!path.exists());
        }
    }
}
