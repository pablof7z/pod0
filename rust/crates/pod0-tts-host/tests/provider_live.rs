use std::{path::PathBuf, time::Duration};

use pod0_tts_host::{
    CancellationToken, ElevenLabsEndpoint, GenerationRequest, ProviderSecret, TtsClient, TtsLimits,
};

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this ignored live test"))
}

#[tokio::test]
#[ignore = "requires POD0_LIVE_ELEVENLABS_API_KEY, POD0_LIVE_ELEVENLABS_MODEL_ID, and POD0_LIVE_ELEVENLABS_VOICE_ID"]
async fn generates_audio_with_a_live_elevenlabs_compatible_provider() {
    let base_url = std::env::var("POD0_LIVE_ELEVENLABS_BASE_URL")
        .unwrap_or_else(|_| "https://api.elevenlabs.io".to_owned());
    let endpoint = ElevenLabsEndpoint::new(&base_url).expect("valid live provider base URL");
    let secret = ProviderSecret::new(required_env("POD0_LIVE_ELEVENLABS_API_KEY"));
    let model = required_env("POD0_LIVE_ELEVENLABS_MODEL_ID");
    let voice = required_env("POD0_LIVE_ELEVENLABS_VOICE_ID");
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".test-artifacts");
    std::fs::create_dir_all(&directory).expect("create project-local live test directory");
    let output = directory.join(format!("live-{}.mp3", std::process::id()));
    let _ = std::fs::remove_file(&output);

    let evidence = TtsClient::default()
        .generate(
            &GenerationRequest {
                endpoint: &endpoint,
                secret: &secret,
                model_id: &model,
                voice_id: &voice,
                script: "This is a live Pod0 text to speech provider test.",
                output_format: Some("mp3_44100_128"),
                staged_path: &output,
                timeout: Duration::from_secs(60),
                limits: TtsLimits {
                    maximum_script_bytes: 1_024,
                    maximum_output_bytes: 16 * 1_024 * 1_024,
                },
            },
            &CancellationToken::new(),
        )
        .await
        .expect("live provider must return actual audio");

    assert!(evidence.byte_count > 0);
    assert_eq!(
        std::fs::metadata(&output).unwrap().len(),
        evidence.byte_count
    );
    std::fs::remove_file(&output).expect("remove live test output");
    let _ = std::fs::remove_dir(directory);
}
