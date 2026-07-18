import Foundation
import Pod0Core

extension AppStateStore {
    /// Upserts an episode for temporary synthetic/unknown Swift ownership.
    /// RSS parents are read-only here once the shared library is authoritative.
    @discardableResult
    func upsertEpisode(
        podcastID: UUID,
        audioURL: URL,
        title: String,
        imageURL: URL?,
        duration: TimeInterval?
    ) -> Episode {
        if isSharedLibraryAuthoritative,
           !isApplyingSharedLibraryProjection,
           podcast(id: podcastID)?.kind == .rss {
            if let existing = state.episodes.first(where: {
                $0.podcastID == podcastID && $0.guid == audioURL.absoluteString
            }) {
                return existing
            }
            return makeExternalEpisode(
                podcastID: podcastID,
                audioURL: audioURL,
                title: title,
                imageURL: imageURL,
                duration: duration
            )
        }
        let guid = audioURL.absoluteString
        if let idx = state.episodes.firstIndex(where: {
            $0.podcastID == podcastID && $0.guid == guid
        }) {
            var updated = state.episodes[idx]
            if let imageURL { updated.imageURL = imageURL }
            if let duration { updated.duration = duration }
            if updated != state.episodes[idx] {
                mutateState { $0.episodes[idx] = updated }
            }
            return state.episodes[idx]
        }
        let episode = makeExternalEpisode(
            podcastID: podcastID,
            audioURL: audioURL,
            title: title,
            imageURL: imageURL,
            duration: duration
        )
        performMutationBatch {
            mutateState { $0.episodes.append(episode) }
            invalidateEpisodeProjections()
        }
        WorkflowRuntime.shared.wake()
        return episode
    }

    func upsertExternalEpisodeAndWait(
        podcastID: UUID,
        feedURL: URL?,
        podcastTitle: String,
        audioURL: URL,
        title: String,
        imageURL: URL?,
        duration: TimeInterval?
    ) async throws -> Episode {
        if isSharedLibraryAuthoritative,
           feedURL != nil || podcast(id: podcastID)?.kind == .rss {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            let durationMilliseconds = duration.flatMap { value -> UInt64? in
                guard value.isFinite, value >= 0 else { return nil }
                return UInt64(min(value * 1_000, Double(UInt64.max)).rounded())
            }
            let result = try await sharedLibrary.execute(.upsertExternalEpisode(
                podcastId: PodcastId(uuid: podcastID),
                feedUrl: feedURL?.absoluteString,
                podcastTitle: podcastTitle,
                audioUrl: audioURL.absoluteString,
                title: title,
                imageUrl: imageURL?.absoluteString,
                durationMilliseconds: durationMilliseconds
            ))
            guard case .externalEpisode(_, let episodeID) = result,
                  let uuid = episodeID.uuid,
                  let episode = episode(id: uuid)
            else { throw SharedLibraryError.unavailable }
            return episode
        }
        return upsertEpisode(
            podcastID: podcastID,
            audioURL: audioURL,
            title: title,
            imageURL: imageURL,
            duration: duration
        )
    }

    private func makeExternalEpisode(
        podcastID: UUID,
        audioURL: URL,
        title: String,
        imageURL: URL?,
        duration: TimeInterval?
    ) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: audioURL.absoluteString,
            title: title,
            pubDate: Date(),
            duration: duration,
            enclosureURL: audioURL,
            imageURL: imageURL
        )
    }
}
