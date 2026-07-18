import Foundation

/// Stable, privacy-safe playback failure. Raw AVFoundation/provider text never
/// reaches SwiftUI or durable state.
struct EngineError: Error, Equatable, Sendable, CustomStringConvertible {
    let failure: ProductFailure

    init(_ error: Error?) {
        failure = Self.classify(error)
    }

    init(failure: ProductFailure) {
        self.failure = failure
    }

    var description: String { failure.diagnosticSummary }

    private static func classify(_ error: Error?) -> ProductFailure {
        guard let error else {
            return ProductFailure(code: .unexpected, diagnosticID: ProductFailure.makeDiagnosticID())
        }
        let nsError = error as NSError
        if let underlying = nsError.userInfo[NSUnderlyingErrorKey] as? Error {
            let classified = classify(underlying)
            if classified.code != .unexpected { return classified }
        }
        if nsError.domain == NSURLErrorDomain {
            return ProductFailure.classify(URLError(URLError.Code(rawValue: nsError.code)))
        }
        return ProductFailure.classify(error)
    }
}
