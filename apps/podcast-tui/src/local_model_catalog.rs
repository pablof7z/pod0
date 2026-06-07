use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub(crate) struct LocalModelCatalog {
    #[serde(default)]
    pub(crate) models: Vec<LocalModelSpec>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct LocalModelSpec {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) description: String,
    pub(crate) size_bytes: i64,
    pub(crate) download_url: String,
    pub(crate) min_device_ram_gb: i64,
}

#[derive(Debug, Deserialize)]
struct LocalModelCatalogEnvelope {
    result: Option<LocalModelCatalog>,
    error: Option<String>,
}

pub(crate) fn decode_local_model_catalog(json: &str) -> Result<LocalModelCatalog, String> {
    let envelope: LocalModelCatalogEnvelope =
        serde_json::from_str(json).map_err(|e| format!("local model catalog JSON: {e}"))?;
    if let Some(error) = envelope.error {
        return Err(error);
    }
    envelope
        .result
        .ok_or_else(|| "local model catalog response missing result".to_owned())
}

pub(crate) fn local_model_summary(id: &str, models: &[LocalModelSpec]) -> String {
    models
        .iter()
        .find(|model| model.id == id)
        .map(|model| format!("{} ({})", model.display_name, model.id))
        .unwrap_or_else(|| id.to_owned())
}

pub(crate) fn local_models_hint(models: &[LocalModelSpec]) -> String {
    if models.is_empty() {
        return "format: model id, blank clears".to_owned();
    }
    let known = models
        .iter()
        .map(|model| format!("{}={}", model.display_name, model.id))
        .collect::<Vec<_>>()
        .join(", ");
    format!("format: model id, blank clears; known: {known}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_catalog_envelope() {
        let catalog = decode_local_model_catalog(
            r#"{"result":{"models":[{"id":"gemma4-e2b","display_name":"Gemma 4 E2B","description":"Small","size_bytes":2590000000,"download_url":"https://example.test/model.litertlm","min_device_ram_gb":4}]}}"#,
        )
        .unwrap();
        assert_eq!(catalog.models[0].id, "gemma4-e2b");
        assert_eq!(
            local_model_summary("gemma4-e2b", &catalog.models),
            "Gemma 4 E2B (gemma4-e2b)"
        );
    }
}
