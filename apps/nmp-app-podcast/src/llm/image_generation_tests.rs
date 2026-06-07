use super::*;

#[test]
fn routes_legacy_image_models_to_images_endpoint() {
    assert!(!uses_chat_completions("openai/dall-e-3"));
    assert!(!uses_chat_completions("black-forest-labs/flux-pro"));
    assert!(uses_chat_completions(
        "google/gemini-2.5-flash-image-preview"
    ));
}

#[test]
fn extracts_chat_image_from_images_array() {
    let response: ChatResponse = serde_json::from_str(
        r#"{"choices":[{"message":{"images":[{"image_url":{"url":"data:image/png;base64,AA=="}}]}}]}"#,
    )
    .unwrap();
    assert_eq!(
        chat_image_url(&response),
        Some("data:image/png;base64,AA==".to_string())
    );
}

#[test]
fn extracts_chat_image_from_content_parts() {
    let response: ChatResponse = serde_json::from_str(
        r#"{"choices":[{"message":{"content":[{"type":"text","text":"done"},{"type":"image_url","image_url":{"url":"https://example.com/i.png"}}]}}]}"#,
    )
    .unwrap();
    assert_eq!(
        chat_image_url(&response),
        Some("https://example.com/i.png".to_string())
    );
}

#[test]
fn decodes_data_urls() {
    let decoded = decode_data_url("data:image/png;base64,SGk=")
        .unwrap()
        .unwrap();
    assert_eq!(decoded, b"Hi");
}

#[test]
fn resolves_model_from_shared_settings_when_intent_omits_it() {
    let settings = ImageProviderSettings {
        openrouter_key: Some(" sk-test ".to_string()),
        image_generation_model: " google/gemini-2.5-flash-image-preview ".to_string(),
    };
    let request = ImageGenerationRequest {
        prompt: "cover art".to_string(),
        model: None,
    };

    let resolved = resolve_openrouter_request(&settings, &request).unwrap();

    assert_eq!(resolved.api_key, "sk-test");
    assert_eq!(resolved.model, "google/gemini-2.5-flash-image-preview");
}

#[test]
fn explicit_intent_model_overrides_shared_setting() {
    let settings = ImageProviderSettings {
        openrouter_key: Some("sk-test".to_string()),
        image_generation_model: "google/default-image".to_string(),
    };
    let request = ImageGenerationRequest {
        prompt: "cover art".to_string(),
        model: Some(" openai/dall-e-3 ".to_string()),
    };

    let resolved = resolve_openrouter_request(&settings, &request).unwrap();

    assert_eq!(resolved.model, "openai/dall-e-3");
}

#[test]
fn missing_key_reports_openrouter_credential_error() {
    let settings = ImageProviderSettings {
        openrouter_key: Some("   ".to_string()),
        image_generation_model: "google/gemini-2.5-flash-image-preview".to_string(),
    };
    let request = ImageGenerationRequest {
        prompt: "cover art".to_string(),
        model: None,
    };

    let error = resolve_openrouter_request(&settings, &request).unwrap_err();

    assert_eq!(error.to_string(), "OpenRouter API key is not configured");
}
