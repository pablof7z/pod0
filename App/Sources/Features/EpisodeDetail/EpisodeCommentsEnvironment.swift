import SwiftUI

private struct EpisodeCommentsRepositoryKey: EnvironmentKey {
    static let defaultValue: any EpisodeCommentsRepository = UnavailableEpisodeCommentsRepository()
}

extension EnvironmentValues {
    var episodeCommentsRepository: any EpisodeCommentsRepository {
        get { self[EpisodeCommentsRepositoryKey.self] }
        set { self[EpisodeCommentsRepositoryKey.self] = newValue }
    }
}
