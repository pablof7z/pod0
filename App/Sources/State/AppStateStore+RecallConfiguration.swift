import Pod0Core

extension AppStateStore {
    var recallConfiguration: RecallConfiguration? {
        _ = recallConfigurationRevision
        return sharedLibrary?.recallConfiguration()
    }

    func applySharedRecallConfiguration(_ configuration: RecallConfiguration) {
        recallConfigurationRevision = max(
            recallConfigurationRevision,
            configuration.revision.value
        )
    }

    func updateRecallConfiguration(
        storedEmbeddingModelID: String? = nil,
        rerankerEnabled: Bool? = nil
    ) {
        guard let sharedLibrary else { return }
        Task { @MainActor in
            do {
                try await sharedLibrary.setRecallConfiguration(
                    storedEmbeddingModelID: storedEmbeddingModelID,
                    rerankerEnabled: rerankerEnabled
                )
            } catch {
                Self.logger.error(
                    "Recall configuration update failed: \(error, privacy: .public)"
                )
            }
        }
    }
}
