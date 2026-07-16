import XCTest

final class UserIdentityWiringTests: XCTestCase {
    private var repositoryRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }

    func testIdentityPublishingUsesNMPDirectly() throws {
        let publishing = try source("App/Sources/Services/UserIdentityStore+Publishing.swift")

        XCTAssertTrue(publishing.contains("engine.signEvent"))
        XCTAssertTrue(publishing.contains("engine.publish"))
        XCTAssertTrue(publishing.contains("WriteIntent("))
        XCTAssertTrue(publishing.contains("routing: .authorOutbox"))
        XCTAssertFalse(publishing.contains("NMPNostrSigner"))
        XCTAssertFalse(publishing.contains("FeedbackRelayClient().publish"))
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

    func testProfilePhotoUploadUsesNMPBlossomSurface() throws {
        let publishing = try source("App/Sources/Services/UserIdentityStore+Publishing.swift")
        let photo = try source("App/Sources/Features/Identity/ChangePhotoSheet.swift")

        XCTAssertTrue(publishing.contains("blossomUploadAuthorizationDraft"))
        XCTAssertTrue(publishing.contains("BlossomAuthorization.validate"))
        XCTAssertTrue(publishing.contains("BlossomClient().upload"))
        XCTAssertTrue(photo.contains("identity.uploadProfilePhoto"))
        XCTAssertFalse(photo.contains("identity.signer"))
    }

    private func source(_ path: String) throws -> String {
        try String(contentsOf: repositoryRoot.appendingPathComponent(path), encoding: .utf8)
    }
}
