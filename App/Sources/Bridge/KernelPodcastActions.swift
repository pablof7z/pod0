import Foundation

enum KernelActionNamespace: String {
    case podcast
}

protocol KernelActionPayload: Encodable {
    static var namespace: KernelActionNamespace { get }
}

extension KernelActionPayload {
    func bodyDictionary() throws -> [String: Any] {
        let data = try JSONEncoder().encode(self)
        let object = try JSONSerialization.jsonObject(with: data)
        guard let body = object as? [String: Any] else {
            throw KernelActionPayloadEncodingError.notObject
        }
        return body
    }
}

enum KernelActionPayloadEncodingError: Error, LocalizedError {
    case notObject

    var errorDescription: String? {
        switch self {
        case .notObject:
            return "Kernel action payload did not encode to a JSON object."
        }
    }
}

extension KernelModel {
    @discardableResult
    func dispatch<Action: KernelActionPayload>(_ action: Action) -> DispatchResult {
        do {
            return dispatch(
                namespace: Action.namespace.rawValue,
                body: try action.bodyDictionary()
            )
        } catch {
            let message = "failed to encode kernel action: \(error.localizedDescription)"
            setErrorToast(message)
            return .failure(message)
        }
    }
}

enum PodcastKernelAction {
    struct Subscribe: KernelActionPayload {
        static let namespace: KernelActionNamespace = .podcast
        let op = "subscribe"
        let feedUrl: String

        enum CodingKeys: String, CodingKey {
            case op
            case feedUrl = "feed_url"
        }
    }

    struct EnsurePodcast: KernelActionPayload {
        static let namespace: KernelActionNamespace = .podcast
        let op = "ensure_podcast"
        let feedUrl: String

        enum CodingKeys: String, CodingKey {
            case op
            case feedUrl = "feed_url"
        }
    }

    struct Unsubscribe: KernelActionPayload {
        static let namespace: KernelActionNamespace = .podcast
        let op = "unsubscribe"
        let podcastId: String

        enum CodingKeys: String, CodingKey {
            case op
            case podcastId = "podcast_id"
        }
    }

    struct Refresh: KernelActionPayload {
        static let namespace: KernelActionNamespace = .podcast
        let op = "refresh"
        let podcastId: String

        enum CodingKeys: String, CodingKey {
            case op
            case podcastId = "podcast_id"
        }
    }

    struct RefreshAll: KernelActionPayload {
        static let namespace: KernelActionNamespace = .podcast
        let op = "refresh_all"
    }

    struct CreatePodcast: KernelActionPayload {
        static let namespace: KernelActionNamespace = .podcast
        let op = "create_podcast"
        let podcastId: String
        let title: String
        let description: String
        let author: String
        let feedUrl: String?
        let artworkUrl: String?
        let language: String?
        let categories: [String]
        let visibility: String
        let titleIsPlaceholder: Bool

        enum CodingKeys: String, CodingKey {
            case op
            case podcastId = "podcast_id"
            case title
            case description
            case author
            case feedUrl = "feed_url"
            case artworkUrl = "artwork_url"
            case language
            case categories
            case visibility
            case titleIsPlaceholder = "title_is_placeholder"
        }
    }

    struct AddEpisode: KernelActionPayload {
        static let namespace: KernelActionNamespace = .podcast
        let op = "add_episode"
        let podcastId: String
        let episodeId: String
        let title: String
        let enclosureUrl: String
        let description: String
        let durationSecs: Double?
        let imageUrl: String?
        let chapters: [KernelEpisodeChapterPayload]
        let transcript: String?

        enum CodingKeys: String, CodingKey {
            case op
            case podcastId = "podcast_id"
            case episodeId = "episode_id"
            case title
            case enclosureUrl = "enclosure_url"
            case description
            case durationSecs = "duration_secs"
            case imageUrl = "image_url"
            case chapters
            case transcript
        }
    }
}

struct KernelEpisodeChapterPayload: Encodable, Equatable {
    let startSecs: Double
    let title: String
    let imageUrl: String?
    let sourceEpisodeId: String?

    init(
        startSecs: Double,
        title: String,
        imageUrl: String? = nil,
        sourceEpisodeId: String? = nil
    ) {
        self.startSecs = startSecs
        self.title = title
        self.imageUrl = imageUrl
        self.sourceEpisodeId = sourceEpisodeId
    }

    init(_ chapter: Episode.Chapter) {
        self.init(
            startSecs: chapter.startTime,
            title: chapter.title,
            imageUrl: chapter.imageURL?.absoluteString,
            sourceEpisodeId: chapter.sourceEpisodeID
        )
    }

    enum CodingKeys: String, CodingKey {
        case startSecs = "start_secs"
        case title
        case imageUrl = "image_url"
        case sourceEpisodeId = "source_episode_id"
    }
}
