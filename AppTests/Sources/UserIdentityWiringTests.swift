import XCTest

final class UserIdentityWiringTests: XCTestCase {
    private var repositoryRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }

    func testHumanPublishingFailsClosedBeforeSigningOrEnqueue() throws {
        let publishing = try source("App/Sources/Services/UserIdentityStore+Publishing.swift")
        let identityRoot = try source("App/Sources/Features/Identity/IdentityRootView.swift")
        let receiptCoordinator = repositoryRoot.appendingPathComponent(
            "App/Sources/Services/UserIdentityStore+NMPReceipts.swift"
        )

        XCTAssertTrue(publishing.contains("durableCorrelationUnavailable(issue: 591)"))
        XCTAssertFalse(publishing.contains("engine.signEvent"))
        XCTAssertFalse(publishing.contains("engine.publish"))
        XCTAssertFalse(publishing.contains("WriteIntent("))
        XCTAssertFalse(FileManager.default.fileExists(atPath: receiptCoordinator.path))
        XCTAssertTrue(identityRoot.contains("NMP issue #591"))
        XCTAssertFalse(identityRoot.contains("EditProfileView"))
    }

    func testIdentityCoreHasNoCompatibilitySigner() throws {
        let core = try source("App/Sources/Services/UserIdentityStore.swift")
        let identityNMP = try source("App/Sources/Services/UserIdentityStore+NMP.swift")

        XCTAssertFalse(core.contains("var signer:"))
        XCTAssertFalse(core.contains("_setSignerForTesting"))
        XCTAssertFalse(identityNMP.contains("NMPNostrSigner"))
        XCTAssertFalse(identityNMP.contains("NostrKeyPair.generate"))
        XCTAssertTrue(core.contains("NMP issue #588"))
    }

    func testProfilePhotoUploadIsAbsentWhilePublishingIsBlocked() throws {
        let publishing = try source("App/Sources/Services/UserIdentityStore+Publishing.swift")
        let photo = repositoryRoot.appendingPathComponent(
            "App/Sources/Features/Identity/ChangePhotoSheet.swift"
        )

        XCTAssertFalse(publishing.contains("blossomUploadAuthorizationDraft"))
        XCTAssertFalse(publishing.contains("BlossomClient().upload"))
        XCTAssertFalse(FileManager.default.fileExists(atPath: photo.path))
    }

    private func source(_ path: String) throws -> String {
        try String(contentsOf: repositoryRoot.appendingPathComponent(path), encoding: .utf8)
    }
}
