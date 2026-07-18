import Foundation

extension SubscriptionService.AddError: ProductFailureConvertible {
    var productFailure: ProductFailure {
        let code: ProductFailureCode
        switch self {
        case .invalidURL, .alreadySubscribed: code = .invalidInput
        case .transport: code = .network
        case .http(let status) where status == 429: code = .rateLimited
        case .http(let status) where status == 408 || status == 504 || status >= 500:
            code = .network
        case .http, .parse: code = .unsupportedFormat
        }
        return ProductFailure(code: code)
    }
}
