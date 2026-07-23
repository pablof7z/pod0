import Foundation
import XCTest
@testable import Podcastr

final class VoiceAgentReachabilityTests: XCTestCase {
    func testUnintegratedVoiceAgentIsNotAdvertisedOrMounted() throws {
        let sources = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("App/Sources")
        let root = try String(
            contentsOf: sources.appendingPathComponent("App/RootView.swift"),
            encoding: .utf8
        )
        let manager = try String(
            contentsOf: sources.appendingPathComponent(
                "Voice/AudioConversationManager.swift"
            ),
            encoding: .utf8
        )

        XCTAssertFalse(root.contains("VoiceView("))
        XCTAssertFalse(root.contains("voiceModeRequested"))
        XCTAssertFalse(manager.contains("StubVoiceTurnDelegate"))

        let appIntents = sources.appendingPathComponent("AppIntents")
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: appIntents,
            includingPropertiesForKeys: nil
        ))
        var violations: [String] = []
        for case let file as URL in enumerator where file.pathExtension == "swift" {
            let source = try String(contentsOf: file, encoding: .utf8)
            if source.contains("Talk to my podcasts")
                || source.contains("StartVoiceModeIntent")
            {
                violations.append(file.lastPathComponent)
            }
        }
        XCTAssertTrue(violations.isEmpty, violations.joined(separator: "\n"))
    }
}
