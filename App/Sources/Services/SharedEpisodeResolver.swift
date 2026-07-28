import Foundation

struct ResolvedSharedEpisode: Equatable, Sendable {
    let podcastTitle: String
    let feedURL: URL?
    let audioURL: URL
    let title: String
    let description: String
    let publishedAt: Date
    let enclosureMIMEType: String?
    let imageURL: URL?
    let duration: TimeInterval?
}

struct SharedEpisodeHTTPDocument: Sendable {
    let data: Data
    let finalURL: URL
    let mimeType: String?
    let statusCode: Int
}

struct SharedEpisodeResolver: Sendable {
    typealias Loader = @Sendable (
        _ url: URL,
        _ accept: String,
        _ maximumBytes: Int
    ) async throws -> SharedEpisodeHTTPDocument

    enum ResolveError: Error, LocalizedError, Equatable {
        case invalidURL
        case http(Int)
        case responseTooLarge
        case noPlayableEpisode

        var errorDescription: String? {
            switch self {
            case .invalidURL:
                "That share did not contain a valid web link."
            case .http:
                "Pod0 could not reach that episode page."
            case .responseTooLarge:
                "That episode page was too large for Pod0 to inspect safely."
            case .noPlayableEpisode:
                "Pod0 could not find playable podcast audio at that link."
            }
        }
    }

    private let load: Loader

    init(session: URLSession = .shared) {
        load = { url, accept, maximumBytes in
            var request = URLRequest(url: url)
            request.timeoutInterval = 30
            request.setValue(accept, forHTTPHeaderField: "Accept")
            request.setValue("Podcastr/1.0", forHTTPHeaderField: "User-Agent")
            let (data, response) = try await session.data(for: request)
            guard let finalURL = response.url else { throw ResolveError.invalidURL }
            let status = (response as? HTTPURLResponse)?.statusCode ?? 200
            guard (200..<300).contains(status) else { throw ResolveError.http(status) }
            guard data.count <= maximumBytes else {
                throw ResolveError.responseTooLarge
            }
            return SharedEpisodeHTTPDocument(
                data: data,
                finalURL: finalURL,
                mimeType: response.mimeType,
                statusCode: status
            )
        }
    }

    init(loader: @escaping Loader) {
        load = loader
    }

    func resolve(_ sourceURL: URL) async throws -> ResolvedSharedEpisode {
        guard Self.isWebURL(sourceURL) else { throw ResolveError.invalidURL }
        if Self.looksLikeAudioURL(sourceURL) {
            return directEpisode(audioURL: sourceURL, sourceURL: sourceURL)
        }

        let document = try await load(
            sourceURL,
            "text/html, application/xhtml+xml, application/rss+xml;q=0.9, application/xml;q=0.8",
            5_000_000
        )
        if document.mimeType?.lowercased().hasPrefix("audio/") == true {
            return directEpisode(audioURL: document.finalURL, sourceURL: sourceURL)
        }

        var page = EpisodeWebPageMetadataParser.parse(
            data: document.data,
            baseURL: document.finalURL
        )
        if page.feedURL == nil, let applePodcastID = page.applePodcastID {
            page.feedURL = try? await lookupFeedURL(applePodcastID: applePodcastID)
        }

        if let feedURL = page.feedURL,
           let feedEpisode = try? await resolveFromFeed(
               feedURL: feedURL,
               page: page
           ) {
            return feedEpisode
        }

        guard let audioURL = page.audioURL else {
            throw ResolveError.noPlayableEpisode
        }
        return ResolvedSharedEpisode(
            podcastTitle: page.podcastTitle
                ?? document.finalURL.host()
                ?? "Shared Podcast",
            feedURL: page.feedURL,
            audioURL: Self.withoutFragment(audioURL),
            title: page.episodeTitle
                ?? Self.fallbackTitle(for: page.canonicalURL ?? sourceURL),
            description: page.description ?? "",
            publishedAt: page.publishedAt ?? Date(),
            enclosureMIMEType: page.audioMIMEType,
            imageURL: page.imageURL,
            duration: page.duration
        )
    }

    private func resolveFromFeed(
        feedURL: URL,
        page: EpisodeWebPageMetadata
    ) async throws -> ResolvedSharedEpisode {
        let feedDocument = try await load(
            feedURL,
            "application/rss+xml, application/atom+xml;q=0.9, application/xml;q=0.8",
            10_000_000
        )
        let parsed = try RSSParser().parse(
            data: feedDocument.data,
            feedURL: feedDocument.finalURL
        )
        guard let episode = Self.bestMatch(in: parsed.episodes, page: page) else {
            throw ResolveError.noPlayableEpisode
        }
        return ResolvedSharedEpisode(
            podcastTitle: parsed.podcast.title.isEmpty
                ? (page.podcastTitle ?? feedDocument.finalURL.host() ?? "Shared Podcast")
                : parsed.podcast.title,
            feedURL: feedDocument.finalURL,
            audioURL: episode.enclosureURL,
            title: episode.title.isEmpty
                ? (page.episodeTitle ?? Self.fallbackTitle(for: episode.enclosureURL))
                : episode.title,
            description: episode.description.isEmpty
                ? (page.description ?? "")
                : episode.description,
            publishedAt: episode.pubDate,
            enclosureMIMEType: episode.enclosureMimeType ?? page.audioMIMEType,
            imageURL: episode.imageURL ?? page.imageURL ?? parsed.podcast.imageURL,
            duration: episode.duration ?? page.duration
        )
    }

    private func lookupFeedURL(applePodcastID: String) async throws -> URL {
        var components = URLComponents(string: "https://itunes.apple.com/lookup")!
        components.queryItems = [
            URLQueryItem(name: "id", value: applePodcastID),
            URLQueryItem(name: "entity", value: "podcast")
        ]
        guard let url = components.url else { throw ResolveError.invalidURL }
        let document = try await load(url, "application/json", 1_000_000)
        let response = try JSONDecoder().decode(ITunesLookupResponse.self, from: document.data)
        guard let feedURL = response.results.compactMap(\.feedURL).first else {
            throw ResolveError.noPlayableEpisode
        }
        return feedURL
    }

    private func directEpisode(audioURL: URL, sourceURL: URL) -> ResolvedSharedEpisode {
        ResolvedSharedEpisode(
            podcastTitle: sourceURL.host() ?? "Shared Podcast",
            feedURL: nil,
            audioURL: Self.withoutFragment(audioURL),
            title: Self.fallbackTitle(for: audioURL),
            description: "",
            publishedAt: Date(),
            enclosureMIMEType: Self.mimeType(for: audioURL),
            imageURL: nil,
            duration: nil
        )
    }

    private struct ITunesLookupResponse: Decodable {
        let results: [Result]

        struct Result: Decodable {
            let feedUrl: String?

            var feedURL: URL? {
                feedUrl.flatMap(URL.init(string:))
            }
        }
    }

    private static func bestMatch(
        in episodes: [Episode],
        page: EpisodeWebPageMetadata
    ) -> Episode? {
        if let audioURL = page.audioURL {
            let wanted = comparableURL(audioURL)
            if let match = episodes.first(where: {
                comparableURL($0.enclosureURL) == wanted
            }) {
                return match
            }
        }
        if let guid = page.guid,
           let match = episodes.first(where: { $0.guid == guid }) {
            return match
        }
        if let title = page.episodeTitle {
            let wanted = comparableTitle(title)
            let matches = episodes.filter { comparableTitle($0.title) == wanted }
            if matches.count == 1 { return matches[0] }
            if let publishedAt = page.publishedAt {
                return matches.min {
                    abs($0.pubDate.timeIntervalSince(publishedAt))
                        < abs($1.pubDate.timeIntervalSince(publishedAt))
                }
            }
        }
        return nil
    }

    private static func comparableURL(_ url: URL) -> String {
        withoutFragment(url).absoluteString
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            .lowercased()
    }

    private static func comparableTitle(_ title: String) -> String {
        title
            .folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
            .split(whereSeparator: \.isWhitespace)
            .joined(separator: " ")
    }

    private static func withoutFragment(_ url: URL) -> URL {
        guard var components = URLComponents(
            url: url,
            resolvingAgainstBaseURL: false
        ) else { return url }
        components.fragment = nil
        return components.url ?? url
    }

    private static func fallbackTitle(for url: URL) -> String {
        let stem = url.deletingPathExtension().lastPathComponent
        let readable = stem
            .replacingOccurrences(of: "-", with: " ")
            .replacingOccurrences(of: "_", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return readable.isEmpty ? "Shared episode" : readable
    }

    private static func isWebURL(_ url: URL) -> Bool {
        ["http", "https"].contains(url.scheme?.lowercased() ?? "")
    }

    private static func looksLikeAudioURL(_ url: URL) -> Bool {
        ["mp3", "m4a", "aac", "ogg", "opus", "wav", "mp4"]
            .contains(url.pathExtension.lowercased())
    }

    private static func mimeType(for url: URL) -> String? {
        switch url.pathExtension.lowercased() {
        case "mp3": "audio/mpeg"
        case "m4a", "mp4": "audio/mp4"
        case "aac": "audio/aac"
        case "ogg", "opus": "audio/ogg"
        case "wav": "audio/wav"
        default: nil
        }
    }
}
