import Foundation

enum NostrSignerError: LocalizedError {
    case invalidEventForSigning
    case remoteRejected(String)
    case timedOut
    case notConnected
    case missingPublicKey

    var errorDescription: String? {
        switch self {
        case .invalidEventForSigning: "Could not sign — event payload is invalid."
        case .remoteRejected(let m): "Remote signer rejected the request: \(m)"
        case .timedOut: "Remote signer did not respond in time."
        case .notConnected: "Remote signer is not connected."
        case .missingPublicKey: "Remote signer has not advertised a public key yet."
        }
    }
}
