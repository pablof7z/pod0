import Foundation

extension EpisodeWebPageMetadataParser {
    struct LinkTag {
        let rel: String
        let type: String?
        let href: String
    }

    static func metadata(in html: String) -> [String: String] {
        var values: [String: String] = [:]
        for attributes in tagAttributes(named: "meta", in: html) {
            guard let key = attributes["property"] ?? attributes["name"],
                  let content = attributes["content"]
            else { continue }
            values[key.lowercased()] = decodeHTMLEntities(content)
        }
        return values
    }

    static func links(in html: String) -> [LinkTag] {
        tagAttributes(named: "link", in: html).compactMap { attributes in
            guard let href = attributes["href"] else { return nil }
            return LinkTag(
                rel: (attributes["rel"] ?? "").lowercased(),
                type: attributes["type"],
                href: decodeHTMLEntities(href)
            )
        }
    }

    static func firstAudioSource(in html: String) -> String? {
        for name in ["audio", "source"] {
            if let source = tagAttributes(named: name, in: html)
                .compactMap({ $0["src"] })
                .first {
                return decodeHTMLEntities(source)
            }
        }
        return nil
    }

    static func tagAttributes(
        named name: String,
        in html: String
    ) -> [[String: String]] {
        let pattern = #"<\#(name)\b[^>]*>"#
        guard let expression = try? NSRegularExpression(
            pattern: pattern,
            options: [.caseInsensitive]
        ) else { return [] }
        let range = NSRange(html.startIndex..., in: html)
        return expression.matches(in: html, range: range).compactMap { match in
            guard let tagRange = Range(match.range, in: html) else { return nil }
            return attributes(in: String(html[tagRange]))
        }
    }

    static func attributes(in tag: String) -> [String: String] {
        let pattern = #"([A-Za-z_:][A-Za-z0-9_:.\-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#
        guard let expression = try? NSRegularExpression(pattern: pattern) else {
            return [:]
        }
        let range = NSRange(tag.startIndex..., in: tag)
        var attributes: [String: String] = [:]
        for match in expression.matches(in: tag, range: range) {
            guard let keyRange = Range(match.range(at: 1), in: tag) else { continue }
            let value = (2...4).compactMap { index -> String? in
                guard match.range(at: index).location != NSNotFound,
                      let range = Range(match.range(at: index), in: tag)
                else { return nil }
                return String(tag[range])
            }.first
            attributes[String(tag[keyRange]).lowercased()] = value
        }
        return attributes
    }

    static func podcastEpisodeJSON(in html: String) -> [String: Any]? {
        let pattern = #"<script\b[^>]*type\s*=\s*["']application/ld\+json["'][^>]*>([\s\S]*?)</script>"#
        guard let expression = try? NSRegularExpression(
            pattern: pattern,
            options: [.caseInsensitive]
        ) else { return nil }
        let range = NSRange(html.startIndex..., in: html)
        for match in expression.matches(in: html, range: range) {
            guard let bodyRange = Range(match.range(at: 1), in: html),
                  let data = String(html[bodyRange]).data(using: .utf8),
                  let object = try? JSONSerialization.jsonObject(with: data),
                  let episode = findPodcastEpisode(in: object)
            else { continue }
            return episode
        }
        return nil
    }

    static func findPodcastEpisode(in value: Any) -> [String: Any]? {
        if let dictionary = value as? [String: Any] {
            if typeIncludesPodcastEpisode(dictionary["@type"]) { return dictionary }
            for child in dictionary.values {
                if let found = findPodcastEpisode(in: child) { return found }
            }
        } else if let array = value as? [Any] {
            for child in array {
                if let found = findPodcastEpisode(in: child) { return found }
            }
        }
        return nil
    }

    static func typeIncludesPodcastEpisode(_ value: Any?) -> Bool {
        if let type = value as? String { return type == "PodcastEpisode" }
        return (value as? [String])?.contains("PodcastEpisode") == true
    }

    static func firstMediaURL(in episode: [String: Any]?) -> String? {
        for key in ["associatedMedia", "encoding", "audio"] {
            guard let value = episode?[key] else { continue }
            if let url = mediaURL(in: value) { return url }
        }
        return nil
    }

    static func mediaURL(in value: Any) -> String? {
        if let string = value as? String { return string }
        if let dictionary = value as? [String: Any] {
            for key in ["contentUrl", "embedUrl", "url"] {
                if let string = dictionary[key] as? String { return string }
            }
        }
        if let array = value as? [Any] {
            return array.compactMap(mediaURL(in:)).first
        }
        return nil
    }

    static func jsonStringField(
        _ field: String,
        in html: String,
        after marker: String?
    ) -> String? {
        let searchable: String
        if let marker, let range = html.range(of: marker) {
            searchable = String(html[range.lowerBound...].prefix(60_000))
        } else {
            searchable = html
        }
        let pattern = #""\#(field)"\s*:\s*"((?:\\.|[^"\\])*)""#
        guard let expression = try? NSRegularExpression(pattern: pattern),
              let match = expression.firstMatch(
                in: searchable,
                range: NSRange(searchable.startIndex..., in: searchable)
              ),
              let valueRange = Range(match.range(at: 1), in: searchable)
        else { return nil }
        let raw = String(searchable[valueRange])
        let quoted = "\"\(raw)\""
        return quoted.data(using: .utf8).flatMap {
            try? JSONDecoder().decode(String.self, from: $0)
        }
    }

    static func applePodcastID(in value: String) -> String? {
        let pattern = #"podcasts\.apple\.com[^\s"'<>]*/id(\d{5,})"#
        guard let expression = try? NSRegularExpression(
            pattern: pattern,
            options: [.caseInsensitive]
        ), let match = expression.firstMatch(
            in: value,
            range: NSRange(value.startIndex..., in: value)
        ), let idRange = Range(match.range(at: 1), in: value)
        else { return nil }
        return String(value[idRange])
    }

    static func splitOvercastTitle(
        _ title: String
    ) -> (episode: String, podcast: String)? {
        guard let separator = title.range(of: " — ", options: .backwards) else {
            return nil
        }
        let episode = clean(String(title[..<separator.lowerBound]))
        let podcast = clean(String(title[separator.upperBound...]))
        guard let episode, let podcast else { return nil }
        return (episode, podcast)
    }

    static func nestedString(
        _ dictionary: [String: Any]?,
        path: [String]
    ) -> String? {
        var value: Any? = dictionary
        for key in path {
            value = (value as? [String: Any])?[key]
        }
        return string(value)
    }

    static func string(_ value: Any?) -> String? {
        value as? String
    }

    static func clean(_ value: String?) -> String? {
        guard let value else { return nil }
        let decoded = decodeHTMLEntities(value)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return decoded.isEmpty ? nil : decoded
    }

    static func resolvedURL(_ raw: String?, relativeTo baseURL: URL) -> URL? {
        guard let raw = clean(raw) else { return nil }
        let url: URL?
        if raw.hasPrefix("//") {
            url = URL(string: "\(baseURL.scheme ?? "https"):\(raw)")
        } else {
            url = URL(string: raw, relativeTo: baseURL)?.absoluteURL
        }
        guard let url, ["http", "https"].contains(url.scheme?.lowercased() ?? "") else {
            return nil
        }
        return url
    }

    static func parseDate(_ value: String?) -> Date? {
        guard let value else { return nil }
        if let date = ISO8601DateFormatter().date(from: value) {
            return date
        }
        // Apple's PodcastEpisode `datePublished` is often a bare calendar
        // date ("2026-07-27") with no time component, which
        // ISO8601DateFormatter's default options reject outright.
        return dateOnlyFormatter.date(from: value)
    }

    private static let dateOnlyFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .iso8601)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "UTC")
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter
    }()

    static func parseDuration(_ value: String?) -> TimeInterval? {
        guard let value else { return nil }
        let pattern = #"^PT(?:(\d+(?:\.\d+)?)H)?(?:(\d+(?:\.\d+)?)M)?(?:(\d+(?:\.\d+)?)S)?$"#
        guard let expression = try? NSRegularExpression(pattern: pattern),
              let match = expression.firstMatch(
                in: value,
                range: NSRange(value.startIndex..., in: value)
              )
        else { return nil }
        func number(at index: Int) -> Double {
            guard match.range(at: index).location != NSNotFound,
                  let range = Range(match.range(at: index), in: value)
            else { return 0 }
            return Double(value[range]) ?? 0
        }
        return number(at: 1) * 3_600 + number(at: 2) * 60 + number(at: 3)
    }
}
