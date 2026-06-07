use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpeechModelCatalog {
    pub eleven_labs_stt: Vec<SpeechModelOption>,
    pub open_router_whisper: Vec<SpeechModelOption>,
    pub assembly_ai_stt: Vec<SpeechModelOption>,
    pub eleven_labs_tts: Vec<SpeechModelOption>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SpeechModelOption {
    pub id: String,
    pub label: String,
}

pub fn speech_model_catalog() -> SpeechModelCatalog {
    SpeechModelCatalog {
        eleven_labs_stt: vec![
            option("scribe_v1", "Scribe v1"),
            option("scribe_v1_experimental", "Scribe v1 experimental"),
            option("scribe_v2", "Scribe v2"),
        ],
        open_router_whisper: vec![option("openai/whisper-1", "OpenAI Whisper")],
        assembly_ai_stt: vec![
            option(
                "universal-3-pro,universal-2",
                "Universal 3 Pro + Universal 2",
            ),
            option("universal-3-pro", "Universal 3 Pro"),
            option("universal-2", "Universal 2"),
        ],
        eleven_labs_tts: vec![
            option("eleven_turbo_v2_5", "Turbo v2.5"),
            option("eleven_flash_v2_5", "Flash v2.5"),
            option("eleven_multilingual_v2", "Multilingual v2"),
        ],
    }
}

fn option(id: &str, label: &str) -> SpeechModelOption {
    SpeechModelOption {
        id: id.to_owned(),
        label: label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_platform_parity_options() {
        let catalog = speech_model_catalog();
        assert_eq!(
            catalog
                .eleven_labs_stt
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            vec!["scribe_v1", "scribe_v1_experimental", "scribe_v2"]
        );
        assert_eq!(
            catalog
                .assembly_ai_stt
                .iter()
                .map(|option| option.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "universal-3-pro,universal-2",
                "universal-3-pro",
                "universal-2"
            ]
        );
    }
}
