import Foundation

extension AppStateStore {
    /// Ensure a feed is known to the Rust kernel without following it.
    /// The kernel owns the fetch, podcast upsert and episode ingestion; Swift
    /// waits for the projected row instead of writing local podcast/episode
    /// state that a later projection could clobber.
    @discardableResult
    func kernelEnsurePodcast(
        feedURL: String,
        timeout: Duration = .seconds(30)
    ) async throws -> Podcast {
        guard let kern = kernel else {
            throw SubscriptionService.AddError.transport("Kernel not available")
        }
        let trimmed = feedURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let url = URL(string: trimmed) else {
            throw SubscriptionService.AddError.invalidURL
        }

        let before = podcast(feedURL: url)
        let beforeRefreshedAt = before?.lastRefreshedAt
        kern.dispatch(PodcastKernelAction.EnsurePodcast(feedUrl: trimmed))

        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline {
            if let current = podcast(feedURL: url) {
                if before == nil || current.lastRefreshedAt != beforeRefreshedAt {
                    return current
                }
            }
            try await Task.sleep(for: .milliseconds(300))
        }

        if let current = podcast(feedURL: url) {
            return current
        }
        throw SubscriptionService.AddError.transport(
            "Feed did not appear in library after \(timeout). It may still arrive.")
    }
}
