import Foundation

/// Parses `podcastr://` deep-links into typed ``Link`` values.
///
/// All work is pure URL parsing with no shared state, so no actor isolation is required.
enum DeepLinkHandler {
    /// The set of deep-link destinations recognised by the app.
    enum Link: Equatable {
        /// Opens the Settings sheet.
        case settings
        /// Opens the Feedback sheet.
        case feedback
        /// Opens the AI agent (Ask) tab.
        case agent
        /// Opens the Add Friend sheet pre-filled with the sender's public key and display name.
        /// `npub` is the bech32-encoded public key; `name` is the optional display name.
        case addFriend(npub: String, name: String?)
        /// Opens the Episode Detail surface for the given episode UUID.
        /// Posted by tapped new-episode notifications.
        case episode(UUID)
        /// Opens the Episode Detail surface for the given episode `guid`,
        /// optionally seeking to `startTime` seconds. Built by
        /// `PlayerMoreMenu.episodeDeepLink` and the transcript-segment share
        /// path — both share-out a `podcastr://e/<guid>?t=<sec>` URL that
        /// pasting back into the app needs to resolve cleanly.
        case episodeByGUID(String, startTime: TimeInterval?)
        /// Opens the Show Detail surface for the given subscription UUID.
        case subscription(UUID)
        /// Opens the Episode Detail surface for the episode that owns the
        /// given clip, seeking playback to the clip's start. Built by
        /// `ClipExporter.deepLink(_:)` (`podcastr://clip/<uuid>`). The
        /// clip→episode lookup is performed by the consumer (RootView)
        /// against the local clip store.
        case clip(UUID)
    }

    /// Converts a URL into a ``Link``, or returns `nil` if the URL is not a recognised deep-link.
    static func resolve(_ url: URL) -> Link? {
        guard url.scheme == "podcastr" else { return nil }
        switch url.host {
        case "settings": return .settings
        case "feedback": return .feedback
        case "agent":    return .agent
        case "friend":
            guard url.path == "/add",
                  let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
                  let npub = components.queryItems?.first(where: { $0.name == "npub" })?.value,
                  !npub.isEmpty
            else { return nil }
            let name = components.queryItems?.first(where: { $0.name == "name" })?.value
            return .addFriend(npub: npub, name: name)
        case "episode":
            // `podcastr://episode/<uuid>` — host=episode, path="/<uuid>".
            guard let raw = firstPathComponent(of: url),
                  let uuid = UUID(uuidString: raw)
            else { return nil }
            return .episode(uuid)
        case "e":
            // `podcastr://e/<guid>?t=<seconds>` — short-link format for
            // share/copy paths. `<guid>` is the episode's RSS guid, not a
            // UUID. `t` is optional and clamped to non-negative.
            guard let raw = firstPathComponent(of: url),
                  !raw.isEmpty
            else { return nil }
            let guid = raw.removingPercentEncoding ?? raw
            let components = URLComponents(url: url, resolvingAgainstBaseURL: false)
            let tValue = components?.queryItems?.first(where: { $0.name == "t" })?.value
            let startTime = tValue.flatMap(TimeInterval.init).map { max(0, $0) }
            return .episodeByGUID(guid, startTime: startTime)
        case "subscription":
            guard let raw = firstPathComponent(of: url),
                  let uuid = UUID(uuidString: raw)
            else { return nil }
            return .subscription(uuid)
        case "clip":
            // `podcastr://clip/<uuid>` — clip share path. Resolution to
            // the underlying episode happens at the consumer; we only
            // parse the id here.
            guard let raw = firstPathComponent(of: url),
                  let uuid = UUID(uuidString: raw)
            else { return nil }
            return .clip(uuid)
        default: return nil
        }
    }

    /// Returns the first non-empty path component of `url` (`podcastr://episode/<uuid>` → `<uuid>`).
    private static func firstPathComponent(of url: URL) -> String? {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
        let path = components.percentEncodedPath
        guard path.hasPrefix("/") else { return nil }
        return path
            .dropFirst()
            .split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true)
            .first
            .map(String.init)
    }

    // MARK: - Link builder

    /// Builds an `podcastr://friend/add` URL suitable for sharing in an invite message.
    static func friendInviteURL(npub: String, name: String?) -> URL? {
        var components = URLComponents()
        components.scheme = "podcastr"
        components.host = "friend"
        components.path = "/add"
        var items: [URLQueryItem] = [URLQueryItem(name: "npub", value: npub)]
        if let name, !name.isEmpty {
            items.append(URLQueryItem(name: "name", value: name))
        }
        components.queryItems = items
        return components.url
    }

    /// Builds the canonical share URL for an RSS GUID. GUIDs are path data, not
    /// URL syntax, so encode them as one conservative path component.
    static func episodeGUIDURL(guid: String, startTime: TimeInterval? = nil) -> URL? {
        let trimmed = guid.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              let encodedGUID = trimmed.addingPercentEncoding(withAllowedCharacters: episodeGUIDAllowedCharacters)
        else { return nil }

        var components = URLComponents()
        components.scheme = "podcastr"
        components.host = "e"
        components.percentEncodedPath = "/\(encodedGUID)"
        if let startTime {
            components.queryItems = [
                URLQueryItem(name: "t", value: "\(max(0, Int(startTime)))")
            ]
        }
        return components.url
    }

    static func episodeGUIDDeepLink(guid: String, startTime: TimeInterval? = nil) -> String? {
        episodeGUIDURL(guid: guid, startTime: startTime)?.absoluteString
    }

    private static let episodeGUIDAllowedCharacters = CharacterSet.alphanumerics
        .union(CharacterSet(charactersIn: "-._~"))
}
