import Foundation
import XCTest
@testable import Podcastr

final class PrivacySafeDiagnosticsTests: XCTestCase {
    func testEndpointOmitsEveryCredentialBearingURLComponent() {
        let secret = "https://user:password@Private.Example.com/member/feed.xml"
            + "?token=top-secret#private-fragment"

        let diagnostic = PrivacySafeDiagnostics.endpoint(secret)

        XCTAssertTrue(diagnostic.hasPrefix("private.example.com#"))
        XCTAssertEqual(diagnostic.count, "private.example.com#".count + 12)
        XCTAssertFalse(diagnostic.contains("user"))
        XCTAssertFalse(diagnostic.contains("password"))
        XCTAssertFalse(diagnostic.contains("member"))
        XCTAssertFalse(diagnostic.contains("token"))
        XCTAssertFalse(diagnostic.contains("top-secret"))
        XCTAssertFalse(diagnostic.contains("fragment"))
    }

    func testEndpointDigestIsStableAndDistinguishesDifferentPrivateFeeds() {
        let first = "https://feeds.example.com/private?token=one"
        let second = "https://feeds.example.com/private?token=two"

        XCTAssertEqual(
            PrivacySafeDiagnostics.endpoint(first),
            PrivacySafeDiagnostics.endpoint(first)
        )
        XCTAssertNotEqual(
            PrivacySafeDiagnostics.endpoint(first),
            PrivacySafeDiagnostics.endpoint(second)
        )
    }

    func testShippingLogsCannotPubliclyInterpolateKnownSensitiveValues() throws {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("App/Sources")
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: nil
        ))
        let forbidden = [
            "absoluteString, privacy: .public",
            "suggestion.feed, privacy: .public",
            "String(data: responseData, encoding: .utf8)",
            "responseBody, privacy: .public",
        ]
        var violations: [String] = []
        for case let file as URL in enumerator where file.pathExtension == "swift" {
            let source = try String(contentsOf: file, encoding: .utf8)
            for pattern in forbidden where source.contains(pattern) {
                violations.append("\(file.lastPathComponent): \(pattern)")
            }
        }
        XCTAssertTrue(violations.isEmpty, violations.joined(separator: "\n"))
    }
}
