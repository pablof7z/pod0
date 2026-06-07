package io.f7z.podcast.ui

import io.f7z.podcast.KernelBridge
import io.f7z.podcast.SettingsSnapshot

enum class ProviderModelRole(val title: String) {
    AgentInitial("Agent Initial"),
    AgentThinking("Agent Thinking"),
    Memory("Memory Compilation"),
    Wiki("Wiki"),
    Categorization("Categorization"),
    ChapterCompilation("Chapter Compilation"),
    Embeddings("Embeddings"),
    ImageGeneration("Image Generation");

    fun modelId(settings: SettingsSnapshot): String = when (this) {
        AgentInitial -> settings.agentInitialModel
        AgentThinking -> settings.agentThinkingModel
        Memory -> settings.memoryCompilationModel
        Wiki -> settings.wikiModel
        Categorization -> settings.categorizationModel
        ChapterCompilation -> settings.chapterCompilationModel
        Embeddings -> settings.embeddingsModel
        ImageGeneration -> settings.imageGenerationModel
    }

    fun modelName(settings: SettingsSnapshot): String = when (this) {
        AgentInitial -> settings.agentInitialModelName
        AgentThinking -> settings.agentThinkingModelName
        Memory -> settings.memoryCompilationModelName
        Wiki -> settings.wikiModelName
        Categorization -> settings.categorizationModelName
        ChapterCompilation -> settings.chapterCompilationModelName
        Embeddings -> settings.embeddingsModelName
        ImageGeneration -> settings.imageGenerationModelName
    }

    fun dispatchSelection(bridge: KernelBridge, modelId: String, modelName: String) {
        when (this) {
            AgentInitial -> dispatchModel(bridge, SetAgentInitialModelPayload(modelId, modelName))
            AgentThinking -> dispatchModel(bridge, SetAgentThinkingModelPayload(modelId, modelName))
            Memory -> dispatchModel(bridge, SetMemoryCompilationModelPayload(modelId, modelName))
            Wiki -> dispatchModel(bridge, SetWikiModelPayload(modelId, modelName))
            Categorization -> dispatchModel(bridge, SetCategorizationModelPayload(modelId, modelName))
            ChapterCompilation -> dispatchModel(bridge, SetChapterCompilationModelPayload(modelId, modelName))
            Embeddings -> dispatchModel(bridge, SetEmbeddingsModelPayload(modelId, modelName))
            ImageGeneration -> dispatchModel(bridge, SetImageGenerationModelPayload(modelId, modelName))
        }
    }
}

private inline fun <reified T> dispatchModel(bridge: KernelBridge, payload: T) {
    PodcastActionDispatcher.dispatch(
        bridge = bridge,
        namespace = PodcastNamespace.SETTINGS,
        payload = payload,
    )
}
