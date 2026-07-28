import Foundation

struct EpisodeWebPageMetadata: Equatable, Sendable {
    var episodeTitle: String?
    var podcastTitle: String?
    var description: String?
    var publishedAt: Date?
    var duration: TimeInterval?
    var audioURL: URL?
    var audioMIMEType: String?
    var imageURL: URL?
    var feedURL: URL?
    var canonicalURL: URL?
    var applePodcastID: String?
    var guid: String?
}

enum EpisodeWebPageMetadataParser {
    static func parse(data: Data, baseURL: URL) -> EpisodeWebPageMetadata {
        let html = String(decoding: data, as: UTF8.self)
        let meta = metadata(in: html)
        let linked = links(in: html)
        let jsonEpisode = podcastEpisodeJSON(in: html)
        let episodeID = URLComponents(
            url: baseURL,
            resolvingAgainstBaseURL: false
        )?.queryItems?.first(where: { $0.name == "i" })?.value

        var result = EpisodeWebPageMetadata()
        result.episodeTitle = clean(
            string(jsonEpisode?["name"])
                ?? meta["og:title"]
                ?? meta["twitter:title"]
                ?? meta["apple:title"]
        )
        result.podcastTitle = clean(
            nestedString(jsonEpisode, path: ["partOfSeries", "name"])
                ?? string(jsonEpisode?["productionCompany"])
        )
        result.description = clean(
            string(jsonEpisode?["description"])
                ?? meta["apple:description"]
                ?? meta["og:description"]
                ?? meta["twitter:description"]
        )
        result.publishedAt = parseDate(string(jsonEpisode?["datePublished"]))
        result.duration = parseDuration(string(jsonEpisode?["duration"]))
        result.imageURL = resolvedURL(
            string(jsonEpisode?["thumbnailUrl"])
                ?? meta["og:image"]
                ?? meta["twitter:image"],
            relativeTo: baseURL
        )
        result.canonicalURL = resolvedURL(
            linked.first(where: { $0.rel == "canonical" })?.href
                ?? meta["og:url"],
            relativeTo: baseURL
        )
        result.audioMIMEType = meta["twitter:player:stream:content_type"]
            ?? meta["og:audio:type"]

        let linkedFeed = linked.first {
            $0.rel.contains("alternate")
                && ($0.type?.localizedCaseInsensitiveContains("rss") == true
                    || $0.type?.localizedCaseInsensitiveContains("atom") == true)
        }?.href
        let marker = episodeID.map { "\"contentId\":\"\($0)\"" }
        result.feedURL = resolvedURL(
            linkedFeed ?? jsonStringField("feedUrl", in: html, after: marker),
            relativeTo: baseURL
        )
        result.guid = jsonStringField("guid", in: html, after: marker)

        let structuredAudio = firstMediaURL(in: jsonEpisode)
        let embeddedAudio = jsonStringField("streamUrl", in: html, after: marker)
        let taggedAudio = firstAudioSource(in: html)
        result.audioURL = resolvedURL(
            meta["twitter:player:stream"]
                ?? meta["og:audio"]
                ?? meta["og:audio:url"]
                ?? structuredAudio
                ?? embeddedAudio
                ?? taggedAudio,
            relativeTo: baseURL
        )

        result.applePodcastID = applePodcastID(in: baseURL.absoluteString)
            ?? applePodcastID(in: html)

        if baseURL.host()?.localizedCaseInsensitiveContains("overcast.fm") == true,
           result.podcastTitle == nil,
           let combined = result.episodeTitle,
           let split = splitOvercastTitle(combined) {
            result.episodeTitle = split.episode
            result.podcastTitle = split.podcast
        }
        return result
    }
}
