use nmp_app_podcast::ffi::SettingsSnapshot;

use crate::provider_settings_parser::*;
use crate::runtime::{AppRuntime, Result};
use crate::speech_model_catalog::{option_summary, options_hint, SpeechModelCatalog};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderSettingItem {
    LoadEnvCredentials,
    AgentInitialModel,
    AgentThinkingModel,
    MemoryCompilationModel,
    WikiModel,
    CategorizationModel,
    ChapterCompilationModel,
    EmbeddingsModel,
    ImageGenerationModel,
    RerankerEnabled,
    OpenRouterCredential,
    OllamaCredential,
    OllamaChatUrl,
    ElevenLabsCredential,
    SttProvider,
    SttKeysPresent,
    OpenRouterWhisperModel,
    AssemblyAiSttModel,
    ElevenLabsModels,
    ElevenLabsVoice,
    LocalModel,
}

pub(crate) const PROVIDER_SETTINGS_ITEMS: [ProviderSettingItem; 21] = [
    ProviderSettingItem::LoadEnvCredentials,
    ProviderSettingItem::AgentInitialModel,
    ProviderSettingItem::AgentThinkingModel,
    ProviderSettingItem::MemoryCompilationModel,
    ProviderSettingItem::WikiModel,
    ProviderSettingItem::CategorizationModel,
    ProviderSettingItem::ChapterCompilationModel,
    ProviderSettingItem::EmbeddingsModel,
    ProviderSettingItem::ImageGenerationModel,
    ProviderSettingItem::RerankerEnabled,
    ProviderSettingItem::OpenRouterCredential,
    ProviderSettingItem::OllamaCredential,
    ProviderSettingItem::OllamaChatUrl,
    ProviderSettingItem::ElevenLabsCredential,
    ProviderSettingItem::SttProvider,
    ProviderSettingItem::SttKeysPresent,
    ProviderSettingItem::OpenRouterWhisperModel,
    ProviderSettingItem::AssemblyAiSttModel,
    ProviderSettingItem::ElevenLabsModels,
    ProviderSettingItem::ElevenLabsVoice,
    ProviderSettingItem::LocalModel,
];

impl ProviderSettingItem {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::LoadEnvCredentials => "Load provider keys from env",
            Self::AgentInitialModel => "Agent initial model",
            Self::AgentThinkingModel => "Agent thinking model",
            Self::MemoryCompilationModel => "Memory compilation model",
            Self::WikiModel => "Wiki model",
            Self::CategorizationModel => "Categorization model",
            Self::ChapterCompilationModel => "Chapter compilation model",
            Self::EmbeddingsModel => "Embeddings model",
            Self::ImageGenerationModel => "Image generation model",
            Self::RerankerEnabled => "Reranker",
            Self::OpenRouterCredential => "OpenRouter credential",
            Self::OllamaCredential => "Ollama credential",
            Self::OllamaChatUrl => "Ollama chat URL",
            Self::ElevenLabsCredential => "ElevenLabs credential",
            Self::SttProvider => "STT provider",
            Self::SttKeysPresent => "STT keys present",
            Self::OpenRouterWhisperModel => "OpenRouter Whisper model",
            Self::AssemblyAiSttModel => "AssemblyAI STT model",
            Self::ElevenLabsModels => "ElevenLabs STT/TTS models",
            Self::ElevenLabsVoice => "ElevenLabs voice",
            Self::LocalModel => "Loaded local model",
        }
    }

    pub(crate) fn value(
        self,
        settings: &SettingsSnapshot,
        speech_catalog: &SpeechModelCatalog,
    ) -> String {
        match self {
            Self::LoadEnvCredentials => env_credentials_summary(),
            Self::AgentInitialModel => model_summary(
                &settings.agent_initial_model,
                &settings.agent_initial_model_name,
            ),
            Self::AgentThinkingModel => model_summary(
                &settings.agent_thinking_model,
                &settings.agent_thinking_model_name,
            ),
            Self::MemoryCompilationModel => model_summary(
                &settings.memory_compilation_model,
                &settings.memory_compilation_model_name,
            ),
            Self::WikiModel => model_summary(&settings.wiki_model, &settings.wiki_model_name),
            Self::CategorizationModel => model_summary(
                &settings.categorization_model,
                &settings.categorization_model_name,
            ),
            Self::ChapterCompilationModel => model_summary(
                &settings.chapter_compilation_model,
                &settings.chapter_compilation_model_name,
            ),
            Self::EmbeddingsModel => {
                model_summary(&settings.embeddings_model, &settings.embeddings_model_name)
            }
            Self::ImageGenerationModel => model_summary(
                &settings.image_generation_model,
                &settings.image_generation_model_name,
            ),
            Self::RerankerEnabled => bool_label(settings.reranker_enabled).to_owned(),
            Self::OpenRouterCredential => credential_summary(
                &settings.open_router_credential_source,
                settings.open_router_byok_key_id.as_deref(),
                settings.open_router_byok_key_label.as_deref(),
                settings.open_router_connected_at,
            ),
            Self::OllamaCredential => credential_summary(
                &settings.ollama_credential_source,
                settings.ollama_byok_key_id.as_deref(),
                settings.ollama_byok_key_label.as_deref(),
                settings.ollama_connected_at,
            ),
            Self::OllamaChatUrl => settings.ollama_chat_url.clone(),
            Self::ElevenLabsCredential => credential_summary(
                &settings.eleven_labs_credential_source,
                settings.eleven_labs_byok_key_id.as_deref(),
                settings.eleven_labs_byok_key_label.as_deref(),
                settings.eleven_labs_connected_at,
            ),
            Self::SttProvider => format!(
                "{} (effective {})",
                settings.stt_provider, settings.effective_stt_provider
            ),
            Self::SttKeysPresent => "comma-separated provider raw values".to_owned(),
            Self::OpenRouterWhisperModel => option_summary(
                &settings.open_router_whisper_model,
                &speech_catalog.open_router_whisper,
            ),
            Self::AssemblyAiSttModel => option_summary(
                &settings.assembly_ai_stt_model,
                &speech_catalog.assembly_ai_stt,
            ),
            Self::ElevenLabsModels => format!(
                "{} | {}",
                option_summary(
                    &settings.eleven_labs_stt_model,
                    &speech_catalog.eleven_labs_stt
                ),
                option_summary(
                    &settings.eleven_labs_tts_model,
                    &speech_catalog.eleven_labs_tts
                )
            ),
            Self::ElevenLabsVoice => pair_summary(
                &settings.eleven_labs_voice_id,
                &settings.eleven_labs_voice_name,
            ),
            Self::LocalModel => settings
                .local_model_id
                .clone()
                .unwrap_or_else(|| "none".to_owned()),
        }
    }

    pub(crate) fn input_hint(self, speech_catalog: &SpeechModelCatalog) -> String {
        match self {
            Self::LoadEnvCredentials => "loads env credentials without showing secrets".to_owned(),
            Self::AgentInitialModel
            | Self::AgentThinkingModel
            | Self::MemoryCompilationModel
            | Self::WikiModel
            | Self::CategorizationModel
            | Self::ChapterCompilationModel
            | Self::EmbeddingsModel
            | Self::ImageGenerationModel => "format: model_id | display name".to_owned(),
            Self::RerankerEnabled => "press Enter to toggle".to_owned(),
            Self::OpenRouterCredential | Self::OllamaCredential | Self::ElevenLabsCredential => {
                "format: source | key_id | key_label | connected_at".to_owned()
            }
            Self::OllamaChatUrl => "format: https://host/api/chat".to_owned(),
            Self::SttProvider => {
                "provider: apple_native | elevenlabs_scribe | assemblyai | openrouter_whisper"
                    .to_owned()
            }
            Self::SttKeysPresent => {
                "comma list: elevenlabs_scribe,assemblyai,openrouter_whisper".to_owned()
            }
            Self::OpenRouterWhisperModel => options_hint(&speech_catalog.open_router_whisper),
            Self::AssemblyAiSttModel => options_hint(&speech_catalog.assembly_ai_stt),
            Self::ElevenLabsModels => format!(
                "format: stt_model | tts_model; STT: {}; TTS: {}",
                options_hint(&speech_catalog.eleven_labs_stt),
                options_hint(&speech_catalog.eleven_labs_tts)
            ),
            Self::ElevenLabsVoice => "format: voice_id | voice_name".to_owned(),
            Self::LocalModel => "format: model id, blank clears".to_owned(),
        }
    }

    pub(crate) fn is_immediate(self) -> bool {
        matches!(self, Self::LoadEnvCredentials | Self::RerankerEnabled)
    }

    pub(crate) fn input_value(self, settings: &SettingsSnapshot) -> String {
        match self {
            Self::LoadEnvCredentials | Self::RerankerEnabled => String::new(),
            Self::AgentInitialModel => model_input(
                &settings.agent_initial_model,
                &settings.agent_initial_model_name,
            ),
            Self::AgentThinkingModel => model_input(
                &settings.agent_thinking_model,
                &settings.agent_thinking_model_name,
            ),
            Self::MemoryCompilationModel => model_input(
                &settings.memory_compilation_model,
                &settings.memory_compilation_model_name,
            ),
            Self::WikiModel => model_input(&settings.wiki_model, &settings.wiki_model_name),
            Self::CategorizationModel => model_input(
                &settings.categorization_model,
                &settings.categorization_model_name,
            ),
            Self::ChapterCompilationModel => model_input(
                &settings.chapter_compilation_model,
                &settings.chapter_compilation_model_name,
            ),
            Self::EmbeddingsModel => {
                model_input(&settings.embeddings_model, &settings.embeddings_model_name)
            }
            Self::ImageGenerationModel => model_input(
                &settings.image_generation_model,
                &settings.image_generation_model_name,
            ),
            Self::OpenRouterCredential => credential_input(
                &settings.open_router_credential_source,
                settings.open_router_byok_key_id.as_deref(),
                settings.open_router_byok_key_label.as_deref(),
                settings.open_router_connected_at,
            ),
            Self::OllamaCredential => credential_input(
                &settings.ollama_credential_source,
                settings.ollama_byok_key_id.as_deref(),
                settings.ollama_byok_key_label.as_deref(),
                settings.ollama_connected_at,
            ),
            Self::OllamaChatUrl => settings.ollama_chat_url.clone(),
            Self::ElevenLabsCredential => credential_input(
                &settings.eleven_labs_credential_source,
                settings.eleven_labs_byok_key_id.as_deref(),
                settings.eleven_labs_byok_key_label.as_deref(),
                settings.eleven_labs_connected_at,
            ),
            Self::SttProvider => settings.stt_provider.clone(),
            Self::SttKeysPresent => String::new(),
            Self::OpenRouterWhisperModel => settings.open_router_whisper_model.clone(),
            Self::AssemblyAiSttModel => settings.assembly_ai_stt_model.clone(),
            Self::ElevenLabsModels => format!(
                "{} | {}",
                settings.eleven_labs_stt_model, settings.eleven_labs_tts_model
            ),
            Self::ElevenLabsVoice => {
                format!(
                    "{} | {}",
                    settings.eleven_labs_voice_id, settings.eleven_labs_voice_name
                )
            }
            Self::LocalModel => settings.local_model_id.clone().unwrap_or_default(),
        }
    }

    pub(crate) fn activate_immediate(
        self,
        settings: &SettingsSnapshot,
        runtime: &AppRuntime,
    ) -> Result<String> {
        match self {
            Self::LoadEnvCredentials => load_env_credentials(runtime),
            Self::RerankerEnabled => {
                runtime.set_reranker_enabled(!settings.reranker_enabled)?;
                Ok("reranker updated".to_owned())
            }
            _ => Err("setting requires text input".to_owned()),
        }
    }

    pub(crate) fn apply_input(self, input: &str, runtime: &AppRuntime) -> Result<String> {
        match self {
            Self::AgentInitialModel => {
                let (model, name) = parse_model_input(input)?;
                runtime.set_agent_initial_model(&model, &name)?;
                Ok("agent initial model updated".to_owned())
            }
            Self::AgentThinkingModel => {
                let (model, name) = parse_model_input(input)?;
                runtime.set_agent_thinking_model(&model, &name)?;
                Ok("agent thinking model updated".to_owned())
            }
            Self::MemoryCompilationModel => {
                let (model, name) = parse_model_input(input)?;
                runtime.set_memory_compilation_model(&model, &name)?;
                Ok("memory model updated".to_owned())
            }
            Self::WikiModel => {
                let (model, name) = parse_model_input(input)?;
                runtime.set_wiki_model(&model, &name)?;
                Ok("wiki model updated".to_owned())
            }
            Self::CategorizationModel => {
                let (model, name) = parse_model_input(input)?;
                runtime.set_categorization_model(&model, &name)?;
                Ok("categorization model updated".to_owned())
            }
            Self::ChapterCompilationModel => {
                let (model, name) = parse_model_input(input)?;
                runtime.set_chapter_compilation_model(&model, &name)?;
                Ok("chapter model updated".to_owned())
            }
            Self::EmbeddingsModel => {
                let (model, name) = parse_model_input(input)?;
                runtime.set_embeddings_model(&model, &name)?;
                Ok("embeddings model updated".to_owned())
            }
            Self::ImageGenerationModel => {
                let (model, name) = parse_model_input(input)?;
                runtime.set_image_generation_model(&model, &name)?;
                Ok("image model updated".to_owned())
            }
            Self::OpenRouterCredential => {
                let (source, key_id, key_label, connected_at) = parse_credential_input(input)?;
                runtime.set_open_router_credential(&source, key_id, key_label, connected_at)?;
                Ok("OpenRouter metadata updated".to_owned())
            }
            Self::OllamaCredential => {
                let (source, key_id, key_label, connected_at) = parse_credential_input(input)?;
                runtime.set_ollama_credential(&source, key_id, key_label, connected_at)?;
                Ok("Ollama metadata updated".to_owned())
            }
            Self::OllamaChatUrl => {
                runtime.set_ollama_chat_url(input.trim())?;
                Ok("Ollama chat URL updated".to_owned())
            }
            Self::ElevenLabsCredential => {
                let (source, key_id, key_label, connected_at) = parse_credential_input(input)?;
                runtime.set_eleven_labs_credential(&source, key_id, key_label, connected_at)?;
                Ok("ElevenLabs metadata updated".to_owned())
            }
            Self::SttProvider => {
                let provider = require_nonempty(input, "provider")?;
                runtime.set_stt_provider(&provider)?;
                Ok("STT provider updated".to_owned())
            }
            Self::SttKeysPresent => {
                runtime.set_stt_keys_present(parse_provider_list(input))?;
                Ok("STT key presence updated".to_owned())
            }
            Self::OpenRouterWhisperModel => {
                let model = require_nonempty(input, "model")?;
                runtime.set_open_router_whisper_model(&model)?;
                Ok("OpenRouter Whisper model updated".to_owned())
            }
            Self::AssemblyAiSttModel => {
                let model = require_nonempty(input, "model")?;
                runtime.set_assembly_ai_stt_model(&model)?;
                Ok("AssemblyAI model updated".to_owned())
            }
            Self::ElevenLabsModels => {
                let (stt_model, tts_model) = parse_required_pair(input, "stt_model", "tts_model")?;
                runtime.set_eleven_labs_models(&stt_model, &tts_model)?;
                Ok("ElevenLabs models updated".to_owned())
            }
            Self::ElevenLabsVoice => {
                let (voice_id, voice_name) = parse_pair_allow_blank(input);
                runtime.set_eleven_labs_voice(&voice_id, &voice_name)?;
                Ok("ElevenLabs voice updated".to_owned())
            }
            Self::LocalModel => {
                runtime.set_local_model(optional_string(input))?;
                Ok("local model updated".to_owned())
            }
            Self::LoadEnvCredentials | Self::RerankerEnabled => {
                Err("setting does not accept text input".to_owned())
            }
        }
    }
}

pub(crate) fn load_env_credentials(runtime: &AppRuntime) -> Result<String> {
    let open_router = env_key("OPENROUTER_API_KEY");
    let ollama = env_key("OLLAMA_API_KEY");
    let eleven_labs = env_key("ELEVENLABS_API_KEY");
    let assembly_ai = env_key("ASSEMBLYAI_API_KEY");
    let perplexity = env_key("PERPLEXITY_API_KEY");

    let mut stt = Vec::new();
    if eleven_labs.is_some() {
        stt.push("elevenlabs_scribe".to_owned());
    }
    if assembly_ai.is_some() {
        stt.push("assemblyai".to_owned());
    }
    if open_router.is_some() {
        stt.push("openrouter_whisper".to_owned());
    }
    if open_router.is_none()
        && ollama.is_none()
        && eleven_labs.is_none()
        && assembly_ai.is_none()
        && perplexity.is_none()
    {
        return Err(
            "no provider env keys set; set OLLAMA_API_KEY, OPENROUTER_API_KEY, ELEVENLABS_API_KEY, ASSEMBLYAI_API_KEY, or PERPLEXITY_API_KEY"
                .to_owned(),
        );
    }

    runtime.set_provider_api_keys(
        open_router.clone(),
        ollama.clone(),
        eleven_labs.clone(),
        assembly_ai.clone(),
        perplexity.clone(),
    )?;
    runtime.set_stt_keys_present(stt)?;
    Ok(format!(
        "env credentials loaded ({})",
        env_credentials_summary()
    ))
}
