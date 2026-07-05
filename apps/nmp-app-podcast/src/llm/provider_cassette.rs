//! Deterministic provider cassette schema and replay lookup.
//!
//! Cassettes are redacted JSON fixtures for provider-backed validation
//! scenarios. They deliberately live in the Rust provider namespace so replay
//! uses the same canonical request fingerprint on iOS, Android, TUI, and
//! headless validation paths.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const CASSETTE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderCassette {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub provider: String,
    pub operation: String,
    #[serde(default)]
    pub scenario_refs: Vec<String>,
    #[serde(default)]
    pub nmp_rules: Vec<String>,
    pub request: CassetteRequest,
    pub response: CassetteResponse,
    pub metrics: CassetteMetrics,
    pub replay: ReplayContract,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CassetteRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Value,
    pub body_sha256: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CassetteResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CassetteMetrics {
    pub recorded_latency_ms: u64,
    pub replay_latency_ms: u64,
    pub budget_ms: u64,
    pub acceptable_for_2026_premium: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReplayContract {
    #[serde(default)]
    pub match_fields: Vec<String>,
    #[serde(default)]
    pub redactions: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Clone)]
pub struct CassetteStore {
    cassettes: Vec<ProviderCassette>,
}

#[derive(Debug, Clone)]
pub struct ReplayResponse {
    pub cassette_id: String,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
    pub replay_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CassetteViolation {
    pub path: PathBuf,
    pub message: String,
}

impl CassetteStore {
    pub fn load_dir(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let mut files = Vec::new();
        collect_json_files(path, &mut files).map_err(|e| e.to_string())?;
        files.sort();

        let mut cassettes = Vec::new();
        for file in files {
            let raw = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let cassette: ProviderCassette =
                serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", file.display()))?;
            cassettes.push(cassette);
        }
        Ok(Self { cassettes })
    }

    pub fn len(&self) -> usize {
        self.cassettes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cassettes.is_empty()
    }

    pub fn find(
        &self,
        provider: &str,
        operation: &str,
        method: &str,
        url: &str,
        body: &Value,
    ) -> Option<ReplayResponse> {
        let fingerprint = request_fingerprint(provider, operation, method, url, body);
        self.cassettes
            .iter()
            .find(|cassette| cassette.request.fingerprint == fingerprint)
            .map(|cassette| ReplayResponse {
                cassette_id: cassette.id.clone(),
                status: cassette.response.status,
                headers: cassette.response.headers.clone(),
                body: cassette.response.body.clone(),
                replay_latency_ms: cassette.metrics.replay_latency_ms,
            })
    }
}

pub fn validate_dir(path: impl AsRef<Path>) -> Vec<CassetteViolation> {
    let path = path.as_ref();
    let mut files = Vec::new();
    if let Err(error) = collect_json_files(path, &mut files) {
        return vec![CassetteViolation {
            path: path.to_path_buf(),
            message: error.to_string(),
        }];
    }
    files.sort();

    let mut ids = BTreeSet::new();
    let mut violations = Vec::new();
    for file in files {
        let raw = match fs::read_to_string(&file) {
            Ok(raw) => raw,
            Err(error) => {
                violations.push(CassetteViolation {
                    path: file,
                    message: error.to_string(),
                });
                continue;
            }
        };
        let cassette: ProviderCassette = match serde_json::from_str(&raw) {
            Ok(cassette) => cassette,
            Err(error) => {
                violations.push(CassetteViolation {
                    path: file,
                    message: error.to_string(),
                });
                continue;
            }
        };
        validate_one(&file, &cassette, &raw, &mut ids, &mut violations);
    }
    violations
}

pub fn body_sha256(body: &Value) -> String {
    sha256_hex(canonical_json(body).as_bytes())
}

pub fn request_fingerprint(
    provider: &str,
    operation: &str,
    method: &str,
    url: &str,
    body: &Value,
) -> String {
    let material = format!(
        "{}\n{}\n{}\n{}\n{}",
        provider.trim().to_ascii_lowercase(),
        operation.trim().to_ascii_lowercase(),
        method.trim().to_ascii_uppercase(),
        url.trim(),
        canonical_json(body)
    );
    sha256_hex(material.as_bytes())
}

fn validate_one(
    path: &Path,
    cassette: &ProviderCassette,
    raw: &str,
    ids: &mut BTreeSet<String>,
    violations: &mut Vec<CassetteViolation>,
) {
    let mut check = |ok: bool, message: &str| {
        if !ok {
            violations.push(CassetteViolation {
                path: path.to_path_buf(),
                message: message.to_owned(),
            });
        }
    };

    check(
        cassette.schema_version == CASSETTE_SCHEMA_VERSION,
        "unsupported schema_version",
    );
    check(!cassette.id.trim().is_empty(), "id is required");
    check(ids.insert(cassette.id.clone()), "duplicate id");
    check(!cassette.provider.trim().is_empty(), "provider is required");
    check(
        !cassette.operation.trim().is_empty(),
        "operation is required",
    );
    check(
        !cassette.scenario_refs.is_empty(),
        "scenario_refs are required",
    );
    check(!cassette.nmp_rules.is_empty(), "nmp_rules are required");
    check(
        cassette.nmp_rules.iter().all(|rule| rule.starts_with('D')),
        "nmp_rules must use doctrine ids such as D6 or D8",
    );
    check(
        cassette.request.body_sha256 == body_sha256(&cassette.request.body),
        "body_sha256 does not match request body",
    );
    check(
        cassette.request.fingerprint
            == request_fingerprint(
                &cassette.provider,
                &cassette.operation,
                &cassette.request.method,
                &cassette.request.url,
                &cassette.request.body,
            ),
        "request fingerprint does not match cassette fields",
    );
    check(
        cassette
            .replay
            .match_fields
            .iter()
            .any(|field| field == "body_sha256"),
        "replay.match_fields must include body_sha256",
    );
    check(
        cassette.metrics.recorded_latency_ms <= cassette.metrics.budget_ms
            || !cassette.metrics.acceptable_for_2026_premium,
        "acceptable cassette exceeds its premium latency budget",
    );
    check(
        !contains_secret(raw),
        "cassette appears to contain a secret",
    );
}

fn collect_json_files(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            files.push(path);
        }
    }
    Ok(())
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into()),
        Value::Array(items) => {
            let inner = items
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{inner}]")
        }
        Value::Object(map) => {
            let mut pairs = map.iter().collect::<Vec<_>>();
            pairs.sort_by(|left, right| left.0.cmp(right.0));
            let inner = pairs
                .into_iter()
                .map(|(key, value)| {
                    let key = serde_json::to_string(key).unwrap_or_else(|_| "\"\"".into());
                    format!("{key}:{}", canonical_json(value))
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn contains_secret(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains("bearer sk-")
        || lower.contains("\"authorization\":\"bearer ")
        || lower.contains("openrouter_api_key")
        || lower.contains("elevenlabs_api_key")
        || lower.contains("assemblyai_api_key")
        || lower.contains("perplexity_api_key")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_are_valid() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/provider_cassettes");
        let violations = validate_dir(&dir);
        assert_eq!(violations, Vec::new());
    }

    #[test]
    fn lookup_returns_matching_replay_response() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/provider_cassettes");
        let store = CassetteStore::load_dir(&dir).expect("load cassettes");
        let body = serde_json::json!({
            "messages": [
                {"content": "You answer from the episode transcript only.", "role": "system"},
                {"content": "What is the host's main takeaway?", "role": "user"}
            ],
            "model": "deepseek/deepseek-chat",
            "stream": false
        });
        let response = store
            .find(
                "openrouter",
                "chat_completion",
                "POST",
                "https://openrouter.ai/api/v1/chat/completions",
                &body,
            )
            .expect("replay response");
        assert_eq!(response.status, 200);
        assert_eq!(response.cassette_id, "openrouter-agent-answer-success");
    }
}
