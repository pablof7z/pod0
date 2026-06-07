use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub(crate) struct SpeechModelCatalog {
    #[serde(default)]
    pub(crate) eleven_labs_stt: Vec<SpeechModelOption>,
    #[serde(default)]
    pub(crate) open_router_whisper: Vec<SpeechModelOption>,
    #[serde(default)]
    pub(crate) assembly_ai_stt: Vec<SpeechModelOption>,
    #[serde(default)]
    pub(crate) eleven_labs_tts: Vec<SpeechModelOption>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct SpeechModelOption {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Debug, Deserialize)]
struct SpeechModelCatalogEnvelope {
    result: Option<SpeechModelCatalog>,
    error: Option<String>,
}

pub(crate) fn decode_speech_model_catalog(json: &str) -> Result<SpeechModelCatalog, String> {
    let envelope: SpeechModelCatalogEnvelope =
        serde_json::from_str(json).map_err(|e| format!("speech catalog JSON: {e}"))?;
    if let Some(error) = envelope.error {
        return Err(error);
    }
    envelope
        .result
        .ok_or_else(|| "speech catalog response missing result".to_owned())
}

pub(crate) fn option_summary(id: &str, options: &[SpeechModelOption]) -> String {
    options
        .iter()
        .find(|option| option.id == id)
        .map(|option| format!("{} ({})", option.label, option.id))
        .unwrap_or_else(|| id.to_owned())
}

pub(crate) fn options_hint(options: &[SpeechModelOption]) -> String {
    if options.is_empty() {
        return "format: model id".to_owned();
    }
    let known = options
        .iter()
        .map(|option| format!("{}={}", option.label, option.id))
        .collect::<Vec<_>>()
        .join(", ");
    format!("format: model id; known: {known}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_catalog_envelope() {
        let catalog = decode_speech_model_catalog(
            r#"{"result":{"eleven_labs_stt":[{"id":"scribe_v2","label":"Scribe v2"}]}}"#,
        )
        .unwrap();
        assert_eq!(catalog.eleven_labs_stt[0].id, "scribe_v2");
    }
}
