import Foundation

extension AppStateStore {

    /// Unfollow a podcast and wait until the Rust subscription-status projection
    /// reports that the show is no longer followed.
    @discardableResult
    func kernelUnfollowAndAwait(
        podcastID: UUID,
        timeout: Duration = .seconds(5)
    ) async -> Bool {
        guard let kernel else { return false }
        let dispatch = kernel.dispatch(PodcastKernelAction.Unfollow(podcastId: podcastID.uuidString))
        guard case .accepted = dispatch else { return false }

        return await awaitState(timeout: timeout) { [weak self] () -> Bool? in
            guard let self else { return nil }
            // Arm Observation on the Swift projection tick, then verify the
            // actual follow status through the Rust-owned projection API.
            _ = self.state.subscriptions
            return self.rustIsAlreadySubscribed(feedURL: nil, ownerPubkey: nil, podcastID: podcastID)
                ? nil
                : true
        } ?? false
    }
}
