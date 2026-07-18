import Foundation

extension VoiceNoteSTTError: ProductFailureConvertible {
    var productFailure: ProductFailure {
        let code: ProductFailureCode
        switch self {
        case .micPermissionDenied: code = .permissionDenied
        case .invalidResponse: code = .unexpected
        case .audio: code = .unsupportedFormat
        }
        return ProductFailure(code: code)
    }
}
