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

    nonisolated static func loadRecallConfiguration(
        facade: Pod0Facade
    ) -> RecallConfiguration? {
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
        let facade = facade
        let current = if let cached = recallConfiguration() {
            cached
        } else {
            await Task.detached(priority: .utility) {
                Self.loadRecallConfiguration(facade: facade)
            }.value
        }
        guard let current else {
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
