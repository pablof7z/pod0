//! Shared provider transport for image generation.
//!
//! This module owns OpenRouter request routing and response parsing for image
//! generation. Hosts pass provider intent (prompt + optional model) and
//! receive image bytes; platform-specific blob handling remains outside Rust.

use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::store::PodcastStore;

const OPENROUTER_CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const OPENROUTER_IMAGES_URL: &str = "https://openrouter.ai/api/v1/images/generations";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageGenerationError {
    MissingCredential(String),
    Provider(String),
    MalformedResponse(String),
    Unavailable(String),
}

impl std::fmt::Display for ImageGenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCredential(provider) => write!(f, "{provider} API key is not configured"),
            Self::Provider(msg) => write!(f, "Image provider error: {msg}"),
            Self::MalformedResponse(msg) => write!(f, "Malformed image response: {msg}"),
            Self::Unavailable(msg) => write!(f, "Image provider unavailable: {msg}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageGenerationRequest {
    pub prompt: String,
    pub model: Option<String>,
}

struct ImageProviderSettings {
    openrouter_key: Option<String>,
    image_generation_model: String,
}

impl ImageProviderSettings {
    fn from_store(store: &Arc<Mutex<PodcastStore>>) -> Result<Self, ImageGenerationError> {
        let store = store.lock().map_err(|_| {
            ImageGenerationError::Unavailable("settings store unavailable".to_string())
        })?;
        Ok(Self {
            openrouter_key: store.open_router_api_key().map(str::to_owned),
            image_generation_model: store.image_generation_model().to_owned(),
        })
    }
}

#[derive(Debug)]
struct ResolvedImageGenerationRequest<'a> {
    prompt: &'a str,
    model: &'a str,
    api_key: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedImage {
    pub bytes: Vec<u8>,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ImagesRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    n: u8,
    size: &'a str,
    response_format: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    images: Vec<ChatImage>,
    content: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ChatImage {
    image_url: ImageUrl,
}

#[derive(Deserialize)]
struct ImageUrl {
    url: String,
}

#[derive(Deserialize)]
struct ImagesResponse {
    data: Vec<ImageData>,
}

#[derive(Deserialize)]
struct ImageData {
    b64_json: Option<String>,
    url: Option<String>,
}

fn uses_chat_completions(model: &str) -> bool {
    let lc = model.to_ascii_lowercase();
    if lc.contains("dall-e") {
        return false;
    }
    if lc.starts_with("black-forest-labs/") {
        return false;
    }
    true
}

fn provider_error(status: reqwest::StatusCode, body: String) -> ImageGenerationError {
    ImageGenerationError::Provider(format!("HTTP {status}: {body}"))
}

fn decode_base64(input: &str) -> Result<Vec<u8>, ImageGenerationError> {
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| ImageGenerationError::MalformedResponse(format!("invalid base64 image: {e}")))
}

fn decode_data_url(url: &str) -> Option<Result<Vec<u8>, ImageGenerationError>> {
    let (_, data) = url.split_once(',')?;
    Some(decode_base64(data))
}

fn image_url_from_content(content: Option<&serde_json::Value>) -> Option<String> {
    let items = content?.as_array()?;
    for item in items {
        if item.get("type").and_then(|t| t.as_str()) != Some("image_url") {
            continue;
        }
        if let Some(url) = item
            .get("image_url")
            .and_then(|i| i.get("url"))
            .and_then(|u| u.as_str())
        {
            return Some(url.to_owned());
        }
    }
    None
}

fn chat_image_url(response: &ChatResponse) -> Option<String> {
    let message = &response.choices.first()?.message;
    if let Some(first) = message.images.first() {
        return Some(first.image_url.url.clone());
    }
    image_url_from_content(message.content.as_ref())
}

fn fetch_image_url(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<Vec<u8>, ImageGenerationError> {
    if let Some(decoded) = decode_data_url(url) {
        return decoded;
    }
    let response = client
        .get(url)
        .send()
        .map_err(|e| ImageGenerationError::Unavailable(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_else(|_| status.to_string());
        return Err(provider_error(status, body));
    }
    response
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|e| ImageGenerationError::Unavailable(e.to_string()))
}

fn generate_via_chat(
    client: &reqwest::blocking::Client,
    request: &ResolvedImageGenerationRequest<'_>,
) -> Result<GeneratedImage, ImageGenerationError> {
    let body = ChatRequest {
        model: request.model,
        messages: vec![ChatMessage {
            role: "user",
            content: request.prompt,
        }],
    };
    let response = client
        .post(OPENROUTER_CHAT_URL)
        .bearer_auth(request.api_key)
        .json(&body)
        .send()
        .map_err(|e| ImageGenerationError::Unavailable(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_else(|_| status.to_string());
        return Err(provider_error(status, body));
    }
    let parsed: ChatResponse = response
        .json()
        .map_err(|e| ImageGenerationError::MalformedResponse(e.to_string()))?;
    let url = chat_image_url(&parsed).ok_or_else(|| {
        ImageGenerationError::MalformedResponse("missing chat image URL".to_string())
    })?;
    Ok(GeneratedImage {
        bytes: fetch_image_url(client, &url)?,
    })
}

fn generate_via_images(
    client: &reqwest::blocking::Client,
    request: &ResolvedImageGenerationRequest<'_>,
) -> Result<GeneratedImage, ImageGenerationError> {
    let body = ImagesRequest {
        model: request.model,
        prompt: request.prompt,
        n: 1,
        size: "1024x1024",
        response_format: "b64_json",
    };
    let response = client
        .post(OPENROUTER_IMAGES_URL)
        .bearer_auth(request.api_key)
        .json(&body)
        .send()
        .map_err(|e| ImageGenerationError::Unavailable(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_else(|_| status.to_string());
        return Err(provider_error(status, body));
    }
    let parsed: ImagesResponse = response
        .json()
        .map_err(|e| ImageGenerationError::MalformedResponse(e.to_string()))?;
    let first = parsed
        .data
        .first()
        .ok_or_else(|| ImageGenerationError::MalformedResponse("missing image data".to_string()))?;
    if let Some(b64) = &first.b64_json {
        return Ok(GeneratedImage {
            bytes: decode_base64(b64)?,
        });
    }
    if let Some(url) = &first.url {
        return Ok(GeneratedImage {
            bytes: fetch_image_url(client, url)?,
        });
    }
    Err(ImageGenerationError::MalformedResponse(
        "missing b64_json or url".to_string(),
    ))
}

fn resolve_openrouter_request<'a>(
    settings: &'a ImageProviderSettings,
    request: &'a ImageGenerationRequest,
) -> Result<ResolvedImageGenerationRequest<'a>, ImageGenerationError> {
    let api_key = settings
        .openrouter_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ImageGenerationError::MissingCredential("OpenRouter".to_string()))?;
    let model = request
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| settings.image_generation_model.trim());
    if model.is_empty() {
        return Err(ImageGenerationError::MalformedResponse(
            "model is empty".to_string(),
        ));
    }
    Ok(ResolvedImageGenerationRequest {
        prompt: &request.prompt,
        model,
        api_key,
    })
}

pub fn generate_openrouter_image(
    store: Arc<Mutex<PodcastStore>>,
    request: &ImageGenerationRequest,
) -> Result<GeneratedImage, ImageGenerationError> {
    let settings = ImageProviderSettings::from_store(&store)?;
    let resolved = resolve_openrouter_request(&settings, request)?;
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| ImageGenerationError::Unavailable(e.to_string()))?;
    if uses_chat_completions(resolved.model) {
        generate_via_chat(&client, &resolved)
    } else {
        generate_via_images(&client, &resolved)
    }
}

#[cfg(test)]
#[path = "image_generation_tests.rs"]
mod tests;
