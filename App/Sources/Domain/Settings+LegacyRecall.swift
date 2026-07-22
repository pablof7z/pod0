import Foundation

struct LegacyRecallConfigurationSeed: Equatable, Sendable {
    let storedEmbeddingModelID: String
    let rerankerEnabled: Bool
}

extension Settings {
    var legacyRecallConfigurationSeed: LegacyRecallConfigurationSeed? {
        guard legacyRecallEmbeddingsModel != nil
                || legacyRecallRerankerEnabled != nil else { return nil }
        return LegacyRecallConfigurationSeed(
            storedEmbeddingModelID: legacyRecallEmbeddingsModel ?? "",
            rerankerEnabled: legacyRecallRerankerEnabled ?? false
        )
    }

    mutating func retireLegacyRecallConfiguration() {
        legacyRecallEmbeddingsModel = nil
        legacyRecallEmbeddingsModelName = nil
        legacyRecallRerankerEnabled = nil
    }
}
