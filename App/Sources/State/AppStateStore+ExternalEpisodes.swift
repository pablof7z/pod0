import Foundation
import Pod0Core

extension AppStateStore {
    func upsertExternalEpisodeAndWait(
        podcastID: UUID,
        feedURL: URL?,
        podcastTitle: String,
        audioURL: URL,
        title: String,
        description: String = "",
        publishedAt: Date = Date(),
        enclosureMimeType: String? = nil,
        imageURL: URL?,
        duration: TimeInterval?
    ) async throws -> Episode {
        guard let sharedLibrary else { throw SharedLibraryError.unavailable }
        let durationMilliseconds = duration.flatMap { value -> UInt64? in
            guard value.isFinite, value >= 0 else { return nil }
            return UInt64(min(value * 1_000, Double(UInt64.max)).rounded())
        }
        let result = try await sharedLibrary.execute(.upsertExternalEpisode(episode: .init(
            podcastId: PodcastId(uuid: podcastID),
            feedUrl: feedURL?.absoluteString,
            podcastTitle: podcastTitle,
            audioUrl: audioURL.absoluteString,
            title: title,
            description: description,
            publishedAt: UnixTimestampMilliseconds(date: publishedAt),
            enclosureMimeType: enclosureMimeType,
            imageUrl: imageURL?.absoluteString,
            durationMilliseconds: durationMilliseconds
        )))
        guard case .externalEpisode(_, let episodeID) = result,
              let uuid = episodeID.uuid,
              let episode = episode(id: uuid)
        else { throw SharedLibraryError.unavailable }
        return episode
    }

    func upsertSyntheticPodcastAndWait(
        _ podcast: Podcast,
        creatingNewIdentity: Bool = false
    ) async throws -> Podcast {
        guard podcast.kind == .synthetic else { throw SharedLibraryError.unavailable }
        guard let sharedLibrary else { throw SharedLibraryError.unavailable }
        let result = try await sharedLibrary.execute(.upsertSyntheticPodcast(podcast: .init(
            podcastId: creatingNewIdentity ? nil : PodcastId(uuid: podcast.id),
            title: podcast.title,
            author: podcast.author,
            imageUrl: podcast.imageURL?.absoluteString,
            description: podcast.description,
            language: podcast.language,
            categories: podcast.categories
        )))
        guard case .podcast(let podcastID) = result,
              let uuid = podcastID.uuid,
              let stored = self.podcast(id: uuid)
        else { throw SharedLibraryError.unavailable }
        return stored
    }
}
