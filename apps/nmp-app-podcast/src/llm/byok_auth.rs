//! Shared BYOK OAuth/PKCE helper for provider credential import.
//!
//! Native shells present the browser and persist returned secrets in their
//! secure host store. Rust owns provider scope mapping, PKCE/state generation,
//! callback validation, and token exchange request/response parsing.

use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

const AUTHORIZATION_URL: &str = "https://byok.f7z.io/authorize";
const TOKEN_URL: &str = "https://byok.f7z.io/api/token";
const STATE_BYTE_COUNT: usize = 32;
const CODE_VERIFIER_BYTE_COUNT: usize = 64;
const TOKEN_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub struct ByokAuthorizationIntent {
    pub providers: Vec<String>,
    pub redirect_uri: String,
    pub client_id: String,
    pub app_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ByokPendingAuthorization {
    pub provider: String,
    pub authorization_url: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub state: String,
    pub code_verifier: String,
}

#[derive(Debug, Deserialize)]
pub struct ByokExchangeIntent {
    pub pending: ByokPendingAuthorization,
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ByokProviderToken {
    pub provider: String,
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ByokTokenResponse {
    pub token_type: String,
    pub provider: String,
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<i64>,
    pub providers: Vec<ByokProviderToken>,
}

#[derive(Debug)]
pub enum ByokAuthError {
    AccessDenied,
    EmptyProviders,
    InvalidAuthorizationUrl,
    InvalidCallback,
    InvalidProvider(String),
    InvalidTokenResponse,
    MissingCode,
    NoProviderKeysReturned,
    ProviderMismatch,
    RandomGenerationFailed(String),
    ServerRejectedToken(Option<String>),
    StateMismatch,
    TokenExchangeFailed(String),
}

impl ByokAuthError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::AccessDenied => "access_denied",
            Self::EmptyProviders => "empty_providers",
            Self::InvalidAuthorizationUrl => "invalid_authorization_url",
            Self::InvalidCallback => "invalid_callback",
            Self::InvalidProvider(_) => "invalid_provider",
            Self::InvalidTokenResponse => "invalid_token_response",
            Self::MissingCode => "missing_code",
            Self::NoProviderKeysReturned => "no_provider_keys_returned",
            Self::ProviderMismatch => "unexpected_provider",
            Self::RandomGenerationFailed(_) => "random_generation_failed",
            Self::ServerRejectedToken(_) => "server_rejected_token",
            Self::StateMismatch => "state_mismatch",
            Self::TokenExchangeFailed(_) => "token_exchange_failed",
        }
    }
}

impl fmt::Display for ByokAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessDenied => write!(f, "Access was denied in BYOK"),
            Self::EmptyProviders => write!(f, "BYOK provider list is empty"),
            Self::InvalidAuthorizationUrl => {
                write!(f, "BYOK authorization URL could not be created")
            }
            Self::InvalidCallback => write!(f, "BYOK returned an unexpected callback"),
            Self::InvalidProvider(provider) => write!(f, "unsupported BYOK provider: {provider}"),
            Self::InvalidTokenResponse => write!(f, "BYOK returned an invalid token response"),
            Self::MissingCode => write!(f, "BYOK did not return an authorization code"),
            Self::NoProviderKeysReturned => {
                write!(f, "BYOK did not return any selected provider keys")
            }
            Self::ProviderMismatch => {
                write!(f, "BYOK returned a credential for the wrong provider")
            }
            Self::RandomGenerationFailed(message) => {
                write!(f, "secure random generation failed: {message}")
            }
            Self::ServerRejectedToken(Some(error)) if !error.is_empty() => {
                write!(f, "BYOK rejected the token exchange: {error}")
            }
            Self::ServerRejectedToken(_) => write!(f, "BYOK rejected the token exchange"),
            Self::StateMismatch => write!(f, "BYOK returned an invalid state"),
            Self::TokenExchangeFailed(message) => {
                write!(f, "BYOK token exchange failed: {message}")
            }
        }
    }
}

pub fn make_authorization(
    intent: ByokAuthorizationIntent,
) -> Result<ByokPendingAuthorization, ByokAuthError> {
    let providers = normalize_providers(&intent.providers)?;
    let state = random_base64_url(STATE_BYTE_COUNT)?;
    let code_verifier = random_base64_url(CODE_VERIFIER_BYTE_COUNT)?;
    let code_challenge = sha256_base64_url(&code_verifier);
    let provider = providers
        .iter()
        .map(|provider| provider.id())
        .collect::<Vec<_>>()
        .join(",");
    let scope = providers
        .iter()
        .map(|provider| provider.scope())
        .collect::<Vec<_>>()
        .join(" ");

    let mut url =
        Url::parse(AUTHORIZATION_URL).map_err(|_| ByokAuthError::InvalidAuthorizationUrl)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", intent.client_id.trim())
        .append_pair("app_name", intent.app_name.trim())
        .append_pair("redirect_uri", intent.redirect_uri.trim())
        .append_pair("scope", &scope)
        .append_pair("state", &state)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");

    Ok(ByokPendingAuthorization {
        provider,
        authorization_url: url.to_string(),
        redirect_uri: intent.redirect_uri.trim().to_owned(),
        client_id: intent.client_id.trim().to_owned(),
        state,
        code_verifier,
    })
}

pub async fn exchange_authorization(
    intent: ByokExchangeIntent,
) -> Result<ByokTokenResponse, ByokAuthError> {
    let client = reqwest::Client::builder()
        .timeout(TOKEN_REQUEST_TIMEOUT)
        .build()
        .map_err(|error| ByokAuthError::TokenExchangeFailed(error.to_string()))?;
    let code = authorization_code_from_callback(&intent.pending, &intent.callback_url)?;
    exchange_code_with_client(&client, TOKEN_URL, &intent.pending, &code).await
}

async fn exchange_code_with_client(
    client: &reqwest::Client,
    token_url: &str,
    pending: &ByokPendingAuthorization,
    code: &str,
) -> Result<ByokTokenResponse, ByokAuthError> {
    let response = client
        .post(token_url)
        .json(&ByokTokenRequest {
            grant_type: "authorization_code",
            code,
            code_verifier: &pending.code_verifier,
            client_id: &pending.client_id,
            redirect_uri: &pending.redirect_uri,
        })
        .send()
        .await
        .map_err(|error| ByokAuthError::TokenExchangeFailed(error.to_string()))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| ByokAuthError::TokenExchangeFailed(error.to_string()))?;
    if !status.is_success() {
        let error = serde_json::from_str::<ByokTokenErrorResponse>(&body)
            .ok()
            .and_then(|response| response.error);
        return Err(ByokAuthError::ServerRejectedToken(error));
    }
    let wire: ByokTokenWireResponse =
        serde_json::from_str(&body).map_err(|_| ByokAuthError::InvalidTokenResponse)?;
    normalize_token_response(wire, &pending.provider)
}

fn normalize_token_response(
    wire: ByokTokenWireResponse,
    requested_provider: &str,
) -> Result<ByokTokenResponse, ByokAuthError> {
    if wire.token_type != "raw_api_key" {
        return Err(ByokAuthError::InvalidTokenResponse);
    }
    let requested = requested_providers(requested_provider)?;
    let providers = if let Some(providers) = wire.providers {
        providers
    } else {
        vec![ByokProviderToken {
            provider: wire
                .provider
                .ok_or(ByokAuthError::InvalidTokenResponse)?
                .to_ascii_lowercase(),
            api_key: wire.api_key.ok_or(ByokAuthError::InvalidTokenResponse)?,
            key_id: wire.key_id,
            key_label: wire.key_label,
        }]
    };
    let providers = providers
        .into_iter()
        .map(|token| ByokProviderToken {
            provider: token.provider.to_ascii_lowercase(),
            api_key: token.api_key,
            key_id: token.key_id,
            key_label: token.key_label,
        })
        .filter(|token| {
            requested.contains(token.provider.as_str()) && !token.api_key.trim().is_empty()
        })
        .collect::<Vec<_>>();
    if providers.is_empty() {
        return if requested.len() == 1 {
            Err(ByokAuthError::ProviderMismatch)
        } else {
            Err(ByokAuthError::NoProviderKeysReturned)
        };
    }
    let first = providers
        .first()
        .cloned()
        .ok_or(ByokAuthError::NoProviderKeysReturned)?;
    Ok(ByokTokenResponse {
        token_type: wire.token_type,
        provider: first.provider,
        api_key: first.api_key,
        key_id: first.key_id,
        key_label: first.key_label,
        app_name: wire.app_name,
        issued_at: wire.issued_at,
        providers,
    })
}

fn authorization_code_from_callback(
    pending: &ByokPendingAuthorization,
    callback_url: &str,
) -> Result<String, ByokAuthError> {
    let callback = Url::parse(callback_url).map_err(|_| ByokAuthError::InvalidCallback)?;
    let redirect = Url::parse(&pending.redirect_uri).map_err(|_| ByokAuthError::InvalidCallback)?;
    if callback.scheme() != redirect.scheme() || callback.host_str() != redirect.host_str() {
        return Err(ByokAuthError::InvalidCallback);
    }
    let query = callback
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<std::collections::HashMap<_, _>>();
    if query.get("state").map(String::as_str) != Some(pending.state.as_str()) {
        return Err(ByokAuthError::StateMismatch);
    }
    if query.get("error").map(String::as_str) == Some("access_denied") {
        return Err(ByokAuthError::AccessDenied);
    }
    query
        .get("code")
        .filter(|code| !code.trim().is_empty())
        .cloned()
        .ok_or(ByokAuthError::MissingCode)
}

fn normalize_providers(values: &[String]) -> Result<Vec<ByokProvider>, ByokAuthError> {
    if values.is_empty() {
        return Err(ByokAuthError::EmptyProviders);
    }
    let mut seen = HashSet::new();
    let mut providers = Vec::new();
    for value in values {
        let provider = ByokProvider::parse(value)?;
        if seen.insert(provider.id()) {
            providers.push(provider);
        }
    }
    if providers.is_empty() {
        Err(ByokAuthError::EmptyProviders)
    } else {
        Ok(providers)
    }
}

fn requested_providers(value: &str) -> Result<HashSet<&str>, ByokAuthError> {
    let requested = value
        .split(',')
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
        .map(|provider| ByokProvider::parse(provider).map(|provider| provider.id()))
        .collect::<Result<HashSet<_>, _>>()?;
    if requested.is_empty() {
        Err(ByokAuthError::EmptyProviders)
    } else {
        Ok(requested)
    }
}

fn random_base64_url(byte_count: usize) -> Result<String, ByokAuthError> {
    let mut bytes = vec![0u8; byte_count];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| ByokAuthError::RandomGenerationFailed(error.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn sha256_base64_url(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ByokProvider {
    OpenRouter,
    ElevenLabs,
    AssemblyAI,
    Ollama,
    Perplexity,
}

impl ByokProvider {
    fn parse(value: &str) -> Result<Self, ByokAuthError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "openrouter" => Ok(Self::OpenRouter),
            "elevenlabs" => Ok(Self::ElevenLabs),
            "assemblyai" => Ok(Self::AssemblyAI),
            "ollama" => Ok(Self::Ollama),
            "perplexity" => Ok(Self::Perplexity),
            other => Err(ByokAuthError::InvalidProvider(other.to_owned())),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::OpenRouter => "openrouter",
            Self::ElevenLabs => "elevenlabs",
            Self::AssemblyAI => "assemblyai",
            Self::Ollama => "ollama",
            Self::Perplexity => "perplexity",
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Self::OpenRouter => "key:openrouter",
            Self::ElevenLabs => "key:elevenlabs",
            Self::AssemblyAI => "key:assemblyai",
            Self::Ollama => "key:ollama",
            Self::Perplexity => "key:perplexity",
        }
    }
}

#[derive(Serialize)]
struct ByokTokenRequest<'a> {
    grant_type: &'static str,
    code: &'a str,
    code_verifier: &'a str,
    client_id: &'a str,
    redirect_uri: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct ByokTokenWireResponse {
    token_type: String,
    provider: Option<String>,
    api_key: Option<String>,
    key_id: Option<String>,
    key_label: Option<String>,
    app_name: Option<String>,
    issued_at: Option<i64>,
    providers: Option<Vec<ByokProviderToken>>,
}

#[derive(Deserialize)]
struct ByokTokenErrorResponse {
    error: Option<String>,
}

#[cfg(test)]
#[path = "byok_auth_tests.rs"]
mod tests;
