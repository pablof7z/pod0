import Foundation

/// Stable failure vocabulary shared by durable workflows and native presentation.
/// Provider bodies, request IDs, paths, and tokens must never be stored here.
enum ProductFailureCode: String, CaseIterable, Codable, Sendable {
    case missingCredential
    case permissionDenied
    case rateLimited
    case offline
    case network
    case unsupportedFormat
    case providerRecovery
    case corruptArtifact
    case cancelled
    case invalidInput
    case missingDependency
    case unexpected
}

struct ProductFailure: Error, Equatable, Sendable {
    let code: ProductFailureCode
    let diagnosticID: String?

    init(code: ProductFailureCode, diagnosticID: String? = nil) {
        self.code = code
        self.diagnosticID = diagnosticID
    }

    var diagnosticSummary: String {
        if let diagnosticID {
            return "failure_code=\(code.rawValue) diagnostic_id=\(diagnosticID)"
        }
        return "failure_code=\(code.rawValue)"
    }

    func addingDiagnosticIDIfNeeded(_ diagnosticID: String) -> ProductFailure {
        guard self.diagnosticID == nil else { return self }
        return ProductFailure(code: code, diagnosticID: diagnosticID)
    }

    static func classify(_ error: Error, diagnosticID: String? = nil) -> ProductFailure {
        let fallbackID = diagnosticID ?? makeDiagnosticID()
        if let failure = error as? ProductFailure {
            return failure.addingDiagnosticIDIfNeeded(fallbackID)
        }
        if let convertible = error as? any ProductFailureConvertible {
            return convertible.productFailure.addingDiagnosticIDIfNeeded(fallbackID)
        }
        if error is CancellationError {
            return ProductFailure(code: .cancelled, diagnosticID: fallbackID)
        }
        if let urlError = error as? URLError {
            return ProductFailure(code: code(for: urlError), diagnosticID: fallbackID)
        }
        return ProductFailure(code: .unexpected, diagnosticID: fallbackID)
    }

    static func makeDiagnosticID() -> String {
        String(UUID().uuidString.prefix(8)).uppercased()
    }

    private static func code(for error: URLError) -> ProductFailureCode {
        switch error.code {
        case .cancelled:
            return .cancelled
        case .notConnectedToInternet, .networkConnectionLost, .dataNotAllowed,
             .internationalRoamingOff, .callIsActive:
            return .offline
        case .cannotFindHost, .cannotConnectToHost, .dnsLookupFailed, .timedOut,
             .secureConnectionFailed, .serverCertificateHasBadDate,
             .serverCertificateUntrusted, .serverCertificateHasUnknownRoot,
             .serverCertificateNotYetValid, .clientCertificateRejected,
             .clientCertificateRequired:
            return .network
        default:
            return .network
        }
    }
}

protocol ProductFailureConvertible: Error {
    var productFailure: ProductFailure { get }
}
