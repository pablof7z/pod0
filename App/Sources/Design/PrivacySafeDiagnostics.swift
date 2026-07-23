import CryptoKit
import Foundation

/// Produces content-free identifiers for values that may contain credentials.
///
/// Feed URLs can carry private tokens in user info, paths, queries, or
/// fragments. Diagnostics may correlate repeated failures, but must never
/// reproduce those components.
enum PrivacySafeDiagnostics {
    static func endpoint(_ url: URL?) -> String {
        endpoint(url?.absoluteString)
    }

    static func endpoint(_ rawValue: String?) -> String {
        guard let rawValue, !rawValue.isEmpty else { return "missing" }
        let host = URL(string: rawValue)?.host?.lowercased() ?? "invalid"
        return "\(host)#\(shortDigest(rawValue))"
    }

    private static func shortDigest(_ value: String) -> String {
        SHA256.hash(data: Data(value.utf8))
            .prefix(6)
            .map { String(format: "%02x", $0) }
            .joined()
    }
}
