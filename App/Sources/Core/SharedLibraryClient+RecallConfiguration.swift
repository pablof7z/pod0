import Pod0Core

extension SharedLibraryClient {
    func subscribeToRecallConfiguration(_ subscriber: SharedLibrarySubscriber) {
        recallConfigurationSubscriptionID = facade.subscribe(
            request: ProjectionRequest(
                scope: .recallConfiguration,
                offset: 0,
                maxItems: 1
            ),
            subscriber: subscriber
        )
    }

    func publishRecallConfiguration(to store: AppStateStore) {
        if cachedRecallConfiguration == nil {
            cachedRecallConfiguration = loadRecallConfiguration()
        }
        if let configuration = cachedRecallConfiguration {
            store.applySharedRecallConfiguration(configuration)
        }
    }

    func unsubscribeFromRecallConfiguration() {
        if let recallConfigurationSubscriptionID {
            facade.unsubscribe(subscriptionId: recallConfigurationSubscriptionID)
        }
        recallConfigurationSubscriptionID = nil
    }

    func recallConfiguration() -> RecallConfiguration? {
        cachedRecallConfiguration
    }

    private func loadRecallConfiguration() -> RecallConfiguration? {
        guard case .recallConfiguration(let configuration) = facade.snapshot(
            request: ProjectionRequest(
                scope: .recallConfiguration,
                offset: 0,
                maxItems: 1
            )
        ).projection else { return nil }
        return configuration
    }

    func setRecallConfiguration(
        storedEmbeddingModelID: String? = nil,
        rerankerEnabled: Bool? = nil
    ) async throws {
        guard let current = recallConfiguration() ?? loadRecallConfiguration() else {
            throw SharedLibraryError.unavailable
        }
        cachedRecallConfiguration = current
        _ = try await execute(.setRecallConfiguration(
            expectedConfigurationRevision: current.revision,
            configuration: RecallConfigurationInput(
                storedEmbeddingModelId: storedEmbeddingModelID
                    ?? current.storedEmbeddingModelId,
                rerankerEnabled: rerankerEnabled ?? current.rerankerEnabled
            )
        ))
    }
}
