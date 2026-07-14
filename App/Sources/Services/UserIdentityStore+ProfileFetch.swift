import Foundation

// MARK: - Profile fetching (kind:0)
//
// Fetches the user's real Nostr kind:0 profile from relays after a key is
// adopted and caches it in UserDefaults so subsequent launches don't flash
// "generated → real" during the relay round-trip.

extension UserIdentityStore {

    static let kind0CachePrefix = "io.f7z.podcast.kind0.v1."

    /// Fetch the most-recent kind:0 event for `pubkeyHex` from all profile
    /// relays, parse its content, and update the store's profile fields.
    /// Silently no-ops on network failure so callers need not handle errors.
    func fetchAndCacheProfile(pubkeyHex: String) async {
        var newestEvent: SignedNostrEvent?
        for relayURL in FeedbackRelayClient.profileRelayURLs {
            let client = FeedbackRelayClient(relayURL: relayURL)
            let events = (try? await client.fetchKind0(pubkeyHex: pubkeyHex)) ?? []
            if let event = events.max(by: { $0.created_at < $1.created_at }) {
                if newestEvent == nil || event.created_at > newestEvent!.created_at {
                    newestEvent = event
                }
            }
        }
        guard let event = newestEvent,
              let data = event.content.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return }

        let displayName = nonEmptyString(json["display_name"] as? String)
        let name        = nonEmptyString(json["name"] as? String)
        let about       = nonEmptyString(json["about"] as? String)
        let picture     = nonEmptyString(json["picture"] as? String)

        let payload: [String: String] = [
            "display_name": displayName ?? "",
            "name":         name        ?? "",
            "about":        about       ?? "",
            "picture":      picture     ?? "",
        ]
        if let cacheData = try? JSONSerialization.data(withJSONObject: payload) {
            UserDefaults.standard.set(cacheData, forKey: Self.kind0CachePrefix + pubkeyHex)
        }

        guard self.publicKeyHex == pubkeyHex else { return }
        self.profileDisplayName = displayName
        self.profileName        = name
        self.profileAbout       = about
        self.profilePicture     = picture
    }

    /// Load profile fields from the UserDefaults cache for instant display
    /// before the relay fetch completes. Called synchronously inside `adoptLocal`.
    func loadCachedProfile(for pubkeyHex: String) {
        guard let data = UserDefaults.standard.data(forKey: Self.kind0CachePrefix + pubkeyHex),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: String]
        else { return }
        profileDisplayName = nonEmptyString(json["display_name"])
        profileName        = nonEmptyString(json["name"])
        profileAbout       = nonEmptyString(json["about"])
        profilePicture     = nonEmptyString(json["picture"])
    }

    private func nonEmptyString(_ s: String?) -> String? {
        s.flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }
    }
}
