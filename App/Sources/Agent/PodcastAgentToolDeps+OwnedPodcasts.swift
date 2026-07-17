import Foundation

// MARK: - AgentOwnedPodcastManagerProtocol
//
// Manages agent-created synthetic podcasts: creation, metadata updates, and
// artwork generation (via image-gen + Blossom upload). Implemented by
// `LiveAgentOwnedPodcastManager`, injected via `PodcastAgentToolDeps`.

protocol AgentOwnedPodcastManagerProtocol: Sendable {
    /// Create a new agent-owned synthetic podcast. Returns the podcast row's
    /// stable info.
    func createPodcast(
        title: String,
        description: String,
        author: String,
        imageURL: URL?,
        language: String?,
        categories: [String]
    ) async throws -> AgentOwnedPodcastInfo

    /// Update mutable metadata on an existing agent-owned podcast. Nil params
    /// keep the current value.
    func updatePodcast(
        podcastID: PodcastID,
        title: String?,
        description: String?,
        author: String?,
        imageURL: URL?
    ) async throws -> AgentOwnedPodcastInfo

    /// Delete an agent-owned podcast and all its episodes.
    func deletePodcast(podcastID: PodcastID) async throws

    /// All podcasts owned by this agent (`Podcast.kind == .synthetic`), newest first.
    func listOwnedPodcasts() async -> [AgentOwnedPodcastInfo]

    /// Generate an image from `prompt`, upload it to Blossom, and return the
    /// resulting URL. The caller can then pass it to `createPodcast` /
    /// `updatePodcast` as `imageURL`.
    func generateAndUploadArtwork(prompt: String) async throws -> URL
}

// MARK: - Result types

struct AgentOwnedPodcastInfo: Sendable {
    let podcastID: String
    let title: String
    let description: String
    let author: String
    let imageURL: URL?
    let episodeCount: Int
}
