//! Shared setup for live headless LLM scenarios.

use nmp_ffi::NmpApp;
use serde_json::json;

use crate::harness::dispatch;

const SETTINGS_NS: &str = "podcast.settings";
const OLLAMA_CHAT_URL: &str = "http://localhost:11434/api/chat";
const GLM_MODEL: &str = "ollama:glm-5.1:cloud";
const GLM_MODEL_NAME: &str = "GLM 5.1 Cloud";

pub fn configure_glm_ollama(app: *mut NmpApp) -> Result<(), String> {
    dispatch_setting(
        app,
        json!({"op": "set_ollama_chat_url", "url": OLLAMA_CHAT_URL}),
    )?;
    dispatch_setting(
        app,
        json!({"op": "set_agent_initial_model", "model": GLM_MODEL, "model_name": GLM_MODEL_NAME}),
    )?;
    dispatch_setting(
        app,
        json!({"op": "set_agent_thinking_model", "model": GLM_MODEL, "model_name": GLM_MODEL_NAME}),
    )?;
    dispatch_setting(
        app,
        json!({"op": "set_wiki_model", "model": GLM_MODEL, "model_name": GLM_MODEL_NAME}),
    )?;
    dispatch_setting(
        app,
        json!({"op": "set_categorization_model", "model": GLM_MODEL, "model_name": GLM_MODEL_NAME}),
    )?;
    Ok(())
}

fn dispatch_setting(app: *mut NmpApp, body: serde_json::Value) -> Result<(), String> {
    let result = dispatch(app, SETTINGS_NS, body.clone());
    if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
        return Err(format!("settings dispatch rejected {body}: {err}"));
    }
    Ok(())
}
