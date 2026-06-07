import Foundation

enum OllamaModelCatalogURL {
    private enum Constants {
        static let defaultTagsURL = "https://ollama.com/api/tags"
    }

    /// Derive the /api/tags discovery URL from a /api/chat endpoint string.
    /// Rust owns the actual catalog request; Swift keeps this only for display
    /// and host-validation tests.
    static func tagsURL(from chatURLString: String?) -> URL {
        guard let str = chatURLString,
              let chatURL = URL(string: str),
              let host = chatURL.host else {
            return URL(string: Constants.defaultTagsURL)!
        }
        var components = URLComponents()
        components.scheme = chatURL.scheme ?? "https"
        components.host = host
        components.port = chatURL.port
        let path = chatURL.path
        if path.hasSuffix("/chat") {
            components.path = String(path.dropLast("/chat".count)) + "/tags"
        } else {
            components.path = "/api/tags"
        }
        return components.url ?? URL(string: Constants.defaultTagsURL)!
    }
}
