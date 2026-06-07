use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LocalModelCatalog {
    pub models: Vec<LocalModelSpec>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LocalModelSpec {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub size_bytes: i64,
    pub download_url: String,
    pub min_device_ram_gb: i64,
}

pub fn local_model_catalog() -> LocalModelCatalog {
    LocalModelCatalog {
        models: vec![
            spec(
                "gemma4-e2b",
                "Gemma 4 E2B",
                "Lightweight efficient LLM for on-device inference",
                2_590_000_000,
                "https://huggingface.co/litert-community/gemma-4-E2B-it-litert-lm/resolve/3f25054/gemma-4-E2B-it.litertlm",
                4,
            ),
            spec(
                "gemma4-e4b",
                "Gemma 4 E4B",
                "Larger efficient LLM variant for improved quality",
                3_800_000_000,
                "https://huggingface.co/litert-community/gemma-4-E4B-it-litert-lm/resolve/f7ad3343bd6ebc9607f4dc3bc4f2398bd5749bc5/gemma-4-E4B-it.litertlm",
                6,
            ),
        ],
    }
}

fn spec(
    id: &str,
    display_name: &str,
    description: &str,
    size_bytes: i64,
    download_url: &str,
    min_device_ram_gb: i64,
) -> LocalModelSpec {
    LocalModelSpec {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        description: description.to_owned(),
        size_bytes,
        download_url: download_url.to_owned(),
        min_device_ram_gb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_gemma_download_specs() {
        let catalog = local_model_catalog();
        assert_eq!(
            catalog
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gemma4-e2b", "gemma4-e4b"]
        );
        assert!(catalog.models.iter().all(|model| {
            model.download_url.starts_with("https://huggingface.co/")
                && model.download_url.ends_with(".litertlm")
                && model.size_bytes > 0
                && model.min_device_ram_gb > 0
        }));
    }
}
