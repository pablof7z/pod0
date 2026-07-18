import Foundation

extension UtilityLLMClientError: ProductFailureConvertible {
    var productFailure: ProductFailure {
        let code: ProductFailureCode
        switch self {
        case .missingCredential:
            code = .missingCredential
        case .httpError(let status, _) where status == 401 || status == 403:
            code = .missingCredential
        case .httpError(let status, _) where status == 429:
            code = .rateLimited
        case .httpError(let status, _) where status == 415 || status == 422:
            code = .unsupportedFormat
        case .httpError(let status, _) where status == 408 || status == 504 || status >= 500:
            code = .network
        case .httpError, .malformedResponse:
            code = .unexpected
        }
        return ProductFailure(code: code)
    }
}
