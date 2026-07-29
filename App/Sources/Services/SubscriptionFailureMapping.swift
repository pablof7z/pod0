import Foundation

extension SubscriptionService.AddError: ProductFailureConvertible {
    var productFailure: ProductFailure {
        let code: ProductFailureCode
        switch self {
        case .invalidURL, .alreadySubscribed: code = .invalidInput
        case .transport: code = .network
        case .parse: code = .unsupportedFormat
        }
        return ProductFailure(code: code)
    }
}
