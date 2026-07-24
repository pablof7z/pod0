import Foundation
import Pod0Core
@testable import Podcastr

enum ChapterModelTransportFixtures {
    static func executionRequest(
        systemPrompt: String = "Return chapter JSON.",
        userPrompt: String = "Use this bounded transcript evidence.",
        provider: String = "openrouter",
        model: String = "fixture-model-v1",
        maximumCompletionBytes: UInt64 = 1_048_576
    ) -> ChapterModelExecutionRequest {
        ChapterModelExecutionRequest(
            provider: provider,
            model: model,
            systemPrompt: systemPrompt,
            userPrompt: userPrompt,
            responseFormat: .jsonObject,
            maximumCompletionBytes: maximumCompletionBytes
        )
    }

}
