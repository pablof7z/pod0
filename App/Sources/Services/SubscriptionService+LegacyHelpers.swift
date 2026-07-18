import Foundation

extension SubscriptionService {
    func normalizedURL(from input: String) -> URL? {
        guard !input.isEmpty else { return nil }
        let candidate = input.contains("://") ? input : "https://\(input)"
        guard let url = URL(string: candidate),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https",
              url.host?.isEmpty == false
        else { return nil }
        return url
    }

    func map(_ error: FeedClient.FeedFetchError) -> AddError {
        switch error {
        case .transport(let underlying):
            return .transport(underlying)
        case .http(let status):
            return .http(status)
        case .parse(let parseError):
            let message = parseError.errorDescription
                ?? (parseError as NSError).localizedDescription
            return .parse(message)
        case .missingFeedURL:
            return .invalidURL
        }
    }
}
