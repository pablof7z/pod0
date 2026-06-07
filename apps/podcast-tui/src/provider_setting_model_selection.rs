use crate::provider_settings_catalog::ProviderSettingItem;
use crate::runtime::{AppRuntime, Result};

impl ProviderSettingItem {
    pub(crate) fn is_model_setting(self) -> bool {
        matches!(
            self,
            Self::AgentInitialModel
                | Self::AgentThinkingModel
                | Self::MemoryCompilationModel
                | Self::WikiModel
                | Self::CategorizationModel
                | Self::ChapterCompilationModel
                | Self::EmbeddingsModel
                | Self::ImageGenerationModel
        )
    }

    pub(crate) fn is_image_model_setting(self) -> bool {
        self == Self::ImageGenerationModel
    }

    pub(crate) fn apply_model_selection(
        self,
        model: &str,
        model_name: &str,
        runtime: &AppRuntime,
    ) -> Result<String> {
        match self {
            Self::AgentInitialModel => {
                runtime.set_agent_initial_model(model, model_name)?;
                Ok("agent initial model updated".to_owned())
            }
            Self::AgentThinkingModel => {
                runtime.set_agent_thinking_model(model, model_name)?;
                Ok("agent thinking model updated".to_owned())
            }
            Self::MemoryCompilationModel => {
                runtime.set_memory_compilation_model(model, model_name)?;
                Ok("memory model updated".to_owned())
            }
            Self::WikiModel => {
                runtime.set_wiki_model(model, model_name)?;
                Ok("wiki model updated".to_owned())
            }
            Self::CategorizationModel => {
                runtime.set_categorization_model(model, model_name)?;
                Ok("categorization model updated".to_owned())
            }
            Self::ChapterCompilationModel => {
                runtime.set_chapter_compilation_model(model, model_name)?;
                Ok("chapter model updated".to_owned())
            }
            Self::EmbeddingsModel => {
                runtime.set_embeddings_model(model, model_name)?;
                Ok("embeddings model updated".to_owned())
            }
            Self::ImageGenerationModel => {
                runtime.set_image_generation_model(model, model_name)?;
                Ok("image model updated".to_owned())
            }
            _ => Err("selected provider row does not accept model selection".to_owned()),
        }
    }
}
