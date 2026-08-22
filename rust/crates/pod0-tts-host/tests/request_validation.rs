mod support;

use std::time::Duration;

use pod0_tts_host::{
    CancellationToken, ElevenLabsEndpoint, GenerationRequest, InvalidRequestField,
    InvalidRequestReason, ProviderSecret, TtsClient, TtsError, TtsLimits,
};
use support::TestDirectory;

#[tokio::test]
async fn unrepresentable_timeout_is_a_typed_invalid_request() {
    let directory = TestDirectory::new("maximum-timeout");
    let path = directory.path().join("audio.mp3");
    let endpoint = ElevenLabsEndpoint::new_loopback_http("http://127.0.0.1:9").unwrap();
    let secret = ProviderSecret::new("secret");

    let error = TtsClient::default()
        .generate(
            &GenerationRequest {
                endpoint: &endpoint,
                secret: &secret,
                model_id: "model",
                voice_id: "voice",
                script: "script",
                output_format: None,
                staged_path: &path,
                timeout: Duration::MAX,
                limits: TtsLimits {
                    maximum_script_bytes: 64,
                    maximum_output_bytes: 1_024,
                },
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        TtsError::InvalidRequest(detail)
            if detail.field == InvalidRequestField::Timeout
                && detail.reason == InvalidRequestReason::Invalid
    ));
    directory.assert_empty();
}
