use sha2::{Digest as _, Sha256};

use crate::{ContentDigest, StateRevision};

pub const RECALL_CONFIGURATION_SCHEMA_VERSION: u32 = 1;
pub const RECALL_EMBEDDING_DIMENSIONS: u16 = 1_024;
pub const DEFAULT_RECALL_EMBEDDING_MODEL: &str = "openai/text-embedding-3-large";
pub const DEFAULT_RECALL_RERANK_MODEL: &str = "cohere/rerank-v3.5";
pub const MAX_RECALL_MODEL_ID_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RecallEmbeddingProvider {
    OpenRouter,
    Ollama,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RecallRerankProvider {
    OpenRouter,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RecallConfigurationOrigin {
    Default,
    LegacySwift,
    User,
    Unsupported { wire_code: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallConfigurationInput {
    pub stored_embedding_model_id: String,
    pub reranker_enabled: bool,
}

impl Default for RecallConfigurationInput {
    fn default() -> Self {
        Self {
            stored_embedding_model_id: DEFAULT_RECALL_EMBEDDING_MODEL.to_owned(),
            reranker_enabled: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct RecallConfiguration {
    pub schema_version: u32,
    pub revision: StateRevision,
    pub origin: RecallConfigurationOrigin,
    pub embedding_provider: RecallEmbeddingProvider,
    pub embedding_model: String,
    pub stored_embedding_model_id: String,
    pub embedding_dimensions: u16,
    pub embedding_space_id: ContentDigest,
    pub reranker_enabled: bool,
    pub reranker_provider: Option<RecallRerankProvider>,
    pub reranker_model: Option<String>,
}

impl Default for RecallConfiguration {
    fn default() -> Self {
        Self::validate(
            RecallConfigurationInput::default(),
            StateRevision::INITIAL,
            RecallConfigurationOrigin::Default,
        )
        .expect("the built-in recall configuration must remain valid")
    }
}

impl RecallConfiguration {
    pub fn validate(
        input: RecallConfigurationInput,
        revision: StateRevision,
        origin: RecallConfigurationOrigin,
    ) -> Result<Self, RecallConfigurationError> {
        let stored = input.stored_embedding_model_id.trim();
        if stored.is_empty()
            || stored.len() > MAX_RECALL_MODEL_ID_BYTES
            || stored.chars().any(char::is_control)
        {
            return Err(RecallConfigurationError::InvalidModel);
        }
        let (embedding_provider, embedding_model, stored_embedding_model_id) =
            if let Some(model) = stored.strip_prefix("ollama:") {
                let model = valid_model(model)?;
                (
                    RecallEmbeddingProvider::Ollama,
                    model.to_owned(),
                    format!("ollama:{model}"),
                )
            } else if let Some(model) = stored.strip_prefix("openrouter:") {
                let model = valid_model(model)?;
                (
                    RecallEmbeddingProvider::OpenRouter,
                    model.to_owned(),
                    model.to_owned(),
                )
            } else {
                let model = valid_model(stored)?;
                (
                    RecallEmbeddingProvider::OpenRouter,
                    model.to_owned(),
                    model.to_owned(),
                )
            };
        let embedding_space_id = embedding_space_id(
            embedding_provider,
            &embedding_model,
            RECALL_EMBEDDING_DIMENSIONS,
        );
        Ok(Self {
            schema_version: RECALL_CONFIGURATION_SCHEMA_VERSION,
            revision,
            origin,
            embedding_provider,
            embedding_model,
            stored_embedding_model_id,
            embedding_dimensions: RECALL_EMBEDDING_DIMENSIONS,
            embedding_space_id,
            reranker_enabled: input.reranker_enabled,
            reranker_provider: input
                .reranker_enabled
                .then_some(RecallRerankProvider::OpenRouter),
            reranker_model: input
                .reranker_enabled
                .then(|| DEFAULT_RECALL_RERANK_MODEL.to_owned()),
        })
    }

    #[must_use]
    pub fn legacy_or_default(input: RecallConfigurationInput, revision: StateRevision) -> Self {
        Self::validate(input, revision, RecallConfigurationOrigin::LegacySwift).unwrap_or_else(
            |_| {
                Self::validate(
                    RecallConfigurationInput::default(),
                    revision,
                    RecallConfigurationOrigin::LegacySwift,
                )
                .expect("the built-in recall configuration must remain valid")
            },
        )
    }

    #[must_use]
    pub fn input(&self) -> RecallConfigurationInput {
        RecallConfigurationInput {
            stored_embedding_model_id: self.stored_embedding_model_id.clone(),
            reranker_enabled: self.reranker_enabled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecallConfigurationError {
    InvalidModel,
}

fn valid_model(value: &str) -> Result<&str, RecallConfigurationError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_RECALL_MODEL_ID_BYTES
        || value.chars().any(char::is_control)
    {
        Err(RecallConfigurationError::InvalidModel)
    } else {
        Ok(value)
    }
}

fn embedding_space_id(
    provider: RecallEmbeddingProvider,
    model: &str,
    dimensions: u16,
) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"pod0.recall-embedding-space.v1\0");
    hash.update(match provider {
        RecallEmbeddingProvider::OpenRouter => b"openrouter".as_slice(),
        RecallEmbeddingProvider::Ollama => b"ollama".as_slice(),
        RecallEmbeddingProvider::Unsupported { .. } => b"unsupported".as_slice(),
    });
    hash.update([0]);
    hash.update(model.as_bytes());
    hash.update([0]);
    hash.update(dimensions.to_be_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_selection_and_embedding_identity_are_deterministic() {
        let open_router = RecallConfiguration::validate(
            RecallConfigurationInput {
                stored_embedding_model_id: " openrouter:openai/embed ".into(),
                reranker_enabled: true,
            },
            StateRevision::new(7),
            RecallConfigurationOrigin::User,
        )
        .unwrap();
        assert_eq!(
            open_router.embedding_provider,
            RecallEmbeddingProvider::OpenRouter
        );
        assert_eq!(open_router.embedding_model, "openai/embed");
        assert_eq!(open_router.stored_embedding_model_id, "openai/embed");
        assert_eq!(
            open_router.reranker_provider,
            Some(RecallRerankProvider::OpenRouter)
        );

        let ollama = RecallConfiguration::validate(
            RecallConfigurationInput {
                stored_embedding_model_id: "ollama:qwen3-embedding".into(),
                reranker_enabled: false,
            },
            StateRevision::new(8),
            RecallConfigurationOrigin::User,
        )
        .unwrap();
        assert_eq!(ollama.embedding_provider, RecallEmbeddingProvider::Ollama);
        assert_ne!(open_router.embedding_space_id, ollama.embedding_space_id);
        assert!(ollama.reranker_provider.is_none());
    }

    #[test]
    fn invalid_legacy_values_fall_back_without_leaving_swift_authoritative() {
        let value = RecallConfiguration::legacy_or_default(
            RecallConfigurationInput {
                stored_embedding_model_id: " \n ".into(),
                reranker_enabled: true,
            },
            StateRevision::new(1),
        );
        assert_eq!(value.origin, RecallConfigurationOrigin::LegacySwift);
        assert_eq!(
            value.stored_embedding_model_id,
            DEFAULT_RECALL_EMBEDDING_MODEL
        );
        assert!(!value.reranker_enabled);
    }
}
