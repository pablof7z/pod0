use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;

use serde_json::Value;

use crate::user_data_erasure::UserDataErasureConfirmation;
use crate::user_data_erasure_marker::{MarkerLocation, sync_parent};
use crate::{StorageError, UserDataTargetKind};

const MAX_SETTINGS_BYTES: usize = 256 * 1024;

const SETTINGS_KEYS: &[&str] = &[
    "agentAvatarURLString",
    "agentDisplayName",
    "agentThinkingModel",
    "agentThinkingModelName",
    "assemblyAISTTModel",
    "autoDeleteDownloadsAfterPlayed",
    "autoFallbackToScribe",
    "autoIngestPublisherTranscripts",
    "autoMarkPlayedAtEnd",
    "autoPlayNext",
    "autoSkipAds",
    "blossomServerURL",
    "categorizationModel",
    "categorizationModelName",
    "chapterCompilationModel",
    "chapterCompilationModelName",
    "defaultPlaybackRate",
    "elevenLabsBYOKKeyID",
    "elevenLabsBYOKKeyLabel",
    "elevenLabsConnectedAt",
    "elevenLabsCredentialSource",
    "elevenLabsSTTModel",
    "elevenLabsTTSModel",
    "elevenLabsVoiceID",
    "elevenLabsVoiceName",
    "embeddingsModel",
    "embeddingsModelName",
    "hasCompletedOnboarding",
    "headphoneDoubleTapAction",
    "headphoneTripleTapAction",
    "imageGenerationModel",
    "imageGenerationModelName",
    "llmModel",
    "llmModelName",
    "memoryCompilationModel",
    "memoryCompilationModelName",
    "ollamaBYOKKeyID",
    "ollamaBYOKKeyLabel",
    "ollamaChatURL",
    "ollamaConnectedAt",
    "ollamaCredentialSource",
    "openRouterBYOKKeyID",
    "openRouterBYOKKeyLabel",
    "openRouterConnectedAt",
    "openRouterCredentialSource",
    "openRouterWhisperModel",
    "rerankerEnabled",
    "skipBackwardSeconds",
    "skipForwardSeconds",
    "sttProvider",
    "wikiModel",
    "wikiModelName",
    "youtubeExtractorURL",
];

pub(crate) fn sanitized_application_state(settings_json: &[u8]) -> Result<Vec<u8>, StorageError> {
    if settings_json.len() > MAX_SETTINGS_BYTES {
        return Err(StorageError::CommandConflict);
    }
    let Value::Object(settings) =
        serde_json::from_slice(settings_json).map_err(|_| StorageError::CommandConflict)?
    else {
        return Err(StorageError::CommandConflict);
    };
    let mut retained = BTreeMap::new();
    for (key, value) in settings {
        if SETTINGS_KEYS.binary_search(&key.as_str()).is_err() || !is_scalar(&value) {
            return Err(StorageError::CommandConflict);
        }
        retained.insert(key, value);
    }
    let projection = serde_json::json!({
        "persistenceGeneration": 0,
        "podcasts": [],
        "subscriptions": [],
        "episodes": [],
        "notes": [],
        "categories": [],
        "categorySettings": {},
        "settings": retained,
        "clips": [],
    });
    serde_json::to_vec(&projection).map_err(|_| StorageError::CommandConflict)
}

pub(crate) fn validate_sanitized_application_state(bytes: &[u8]) -> Result<(), StorageError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| StorageError::CommandConflict)?;
    let object = value.as_object().ok_or(StorageError::CommandConflict)?;
    let expected_keys = [
        "categories",
        "categorySettings",
        "clips",
        "episodes",
        "notes",
        "persistenceGeneration",
        "podcasts",
        "settings",
        "subscriptions",
    ];
    if object.len() != expected_keys.len()
        || expected_keys.iter().any(|key| !object.contains_key(*key))
        || object["persistenceGeneration"] != serde_json::json!(0)
        || [
            "categories",
            "clips",
            "episodes",
            "notes",
            "podcasts",
            "subscriptions",
        ]
        .iter()
        .any(|key| object[*key] != serde_json::json!([]))
        || object["categorySettings"] != serde_json::json!({})
    {
        return Err(StorageError::CommandConflict);
    }
    let settings =
        serde_json::to_vec(&object["settings"]).map_err(|_| StorageError::CommandConflict)?;
    let expected = sanitized_application_state(&settings)?;
    let expected: Value =
        serde_json::from_slice(&expected).map_err(|_| StorageError::CommandConflict)?;
    (value == expected)
        .then_some(())
        .ok_or(StorageError::CommandConflict)
}

pub(crate) fn ensure_sanitized_application_state(
    prepared: &UserDataErasureConfirmation,
) -> Result<(), StorageError> {
    let target = prepared
        .marker
        .targets
        .iter()
        .find(|target| target.kind == UserDataTargetKind::ApplicationStateProjection)
        .ok_or(StorageError::CommandConflict)?;
    let MarkerLocation::Filesystem { source, .. } = &target.location else {
        return Err(StorageError::CommandConflict);
    };
    let expected = prepared.marker.sanitized_application_state.as_bytes();
    validate_sanitized_application_state(expected)?;
    if source.exists() {
        return (std::fs::read(source)
            .map_err(|error| StorageError::io("read sanitized application state", error))?
            == expected)
            .then_some(())
            .ok_or(StorageError::CommandConflict);
    }
    write_exact(source, expected)
}

fn write_exact(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let parent = path.parent().ok_or(StorageError::CommandConflict)?;
    std::fs::create_dir_all(parent)
        .map_err(|error| StorageError::io("create sanitized state parent", error))?;
    let temporary = path.with_extension("erasure-next");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| StorageError::io("create sanitized application state", error))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| StorageError::io("sync sanitized application state", error))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| StorageError::io("publish sanitized application state", error))?;
    sync_parent(path)
}

fn is_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_preserves_only_known_scalar_settings_and_has_empty_product_state() {
        let bytes =
            sanitized_application_state(br#"{"llmModel":"model","autoPlayNext":true}"#).unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["settings"]["llmModel"], "model");
        assert_eq!(value["episodes"], serde_json::json!([]));
        assert!(sanitized_application_state(br#"{"episodes":[{"private":true}]}"#).is_err());
        assert!(sanitized_application_state(br#"{"llmModel":{"private":true}}"#).is_err());
        assert!(sanitized_application_state(br#"{"openRouterAPIKey":"secret"}"#).is_err());
        validate_sanitized_application_state(&bytes).unwrap();
        let mut tampered: Value = serde_json::from_slice(&bytes).unwrap();
        tampered["episodes"] = serde_json::json!([{"private": true}]);
        assert!(
            validate_sanitized_application_state(&serde_json::to_vec(&tampered).unwrap()).is_err()
        );
    }
}
