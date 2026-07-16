import XCTest

final class RepositoryArchitectureTests: XCTestCase {
    private var repositoryRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }

    func testProductionAppCannotWireLegacyRemoteAgentIngress() throws {
        let appMain = try source("App/Sources/AppMain.swift")
        let rootView = try source("App/Sources/App/RootView.swift")

        XCTAssertFalse(appMain.contains("NostrRelayService"))
        XCTAssertFalse(rootView.contains("NostrRelayService"))
        XCTAssertEqual(appMain.components(separatedBy: "Pod0NMPComposition(").count - 1, 1)
    }

    func testHumanIdentityStartsFromTheSingleNMPComposition() throws {
        let appMain = try source("App/Sources/AppMain.swift")
        let identityCore = try source("App/Sources/Services/UserIdentityStore.swift")
        let identityNMP = try source("App/Sources/Services/UserIdentityStore+NMP.swift")
        let lifecycle = try source("App/Sources/NMP/Pod0HumanIdentityLifecycle.swift")

        XCTAssertFalse(appMain.contains(".task { userIdentity.start() }"))
        XCTAssertTrue(appMain.contains("await userIdentity.start(composition: composition)"))
        XCTAssertFalse(identityCore.contains("user-private-key-hex"))
        XCTAssertFalse(identityCore.contains("nip46-session"))
        XCTAssertFalse(identityNMP.contains("NostrKeyPair.generate"))
        XCTAssertFalse(identityNMP.contains("func start() async"))
        XCTAssertTrue(identityNMP.contains("nmpKeyGenerationUnavailable"))
        XCTAssertTrue(identityCore.contains("#588"))
        XCTAssertTrue(appMain.contains("NMPKeychainAccountStore("))
        XCTAssertTrue(appMain.contains("localAccountStore:"))
        XCTAssertTrue(appMain.contains(".nmp-human-identity"))
        XCTAssertFalse(lifecycle.contains("loadSecretKey"))
        XCTAssertFalse(lifecycle.contains("saveSecretKey"))
        XCTAssertTrue(lifecycle.contains("issue: 589"))
    }

    func testLegacyHumanSignerTransportIsAbsentFromAppTarget() throws {
        let removed = [
            "App/Sources/Services/Nip46/RemoteSigner.swift",
            "App/Sources/Services/Nip46/RemoteSignerClient.swift",
            "App/Sources/Services/Nip46/RemoteSignerTransport.swift",
            "App/Sources/Services/Nip46/Nip44.swift",
            "App/Sources/Services/UserIdentityStore+NIP46.swift",
        ]
        for path in removed {
            XCTAssertFalse(FileManager.default.fileExists(atPath: repositoryRoot.appendingPathComponent(path).path))
        }
        XCTAssertTrue(FileManager.default.fileExists(
            atPath: repositoryRoot.appendingPathComponent("App/Sources/Agent/AgentNostrSigner.swift").path
        ))
    }

    func testDormantFeedbackSurfaceIsAbsent() throws {
        let rootView = try source("App/Sources/App/RootView.swift")
        let project = try source("Project.swift")
        let profileFetch = repositoryRoot.appendingPathComponent(
            "App/Sources/Services/UserIdentityStore+ProfileFetch.swift"
        )
        let feedbackDirectory = repositoryRoot.appendingPathComponent(
            "App/Sources/Features/Feedback"
        )

        XCTAssertFalse(FileManager.default.fileExists(atPath: profileFetch.path))
        XCTAssertTrue(
            (try? FileManager.default.contentsOfDirectory(atPath: feedbackDirectory.path).isEmpty)
                ?? true
        )
        XCTAssertFalse(rootView.contains("showFeedback"))
        XCTAssertFalse(rootView.contains("onShake"))
        XCTAssertFalse(project.contains("ShakeFeedbackKit"))
    }

    func testCleanCommentsBoundaryNeverUsesCustomSigners() throws {
        let repository = try source("App/Sources/Services/EpisodeCommentsRepository.swift")
        let model = try source("App/Sources/Features/EpisodeDetail/EpisodeCommentsModel.swift")
        let section = try source("App/Sources/Features/EpisodeDetail/EpisodeCommentsSection.swift")
        let combined = repository + model + section

        XCTAssertFalse(combined.contains("UserIdentityStore"))
        XCTAssertFalse(combined.contains("NostrSigner"))
        XCTAssertFalse(combined.contains("LocalKeySigner"))
        XCTAssertFalse(combined.contains("RemoteSigner"))
        XCTAssertTrue(repository.contains("pablof7z/nmp#572"))
    }

    func testRepositoryDependenciesAreSelfContainedAndPinned() throws {
        let project = try source("Project.swift")
        let revision = try source("Vendor/nmp-revision.txt")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let nmpConfiguration = try source("App/Sources/NMP/Pod0NMPConfiguration.swift")

        XCTAssertFalse(project.contains("../"))
        XCTAssertFalse(project.contains("ShakeFeedbackKit"))
        XCTAssertFalse(project.contains("ios-shake-feedback"))
        XCTAssertTrue(project.contains(".local(path: \"Vendor/nmp/Packages/NMP\")"))
        XCTAssertNotNil(revision.range(of: "^[0-9a-f]{40}$", options: .regularExpression))
        XCTAssertTrue(nmpConfiguration.contains("static let testedRevision = \"\(revision)\""))
    }

    func testWorkflowsTargetMaster() throws {
        let tests = try source(".github/workflows/test.yml")
        let testFlight = try source(".github/workflows/testflight.yml")

        XCTAssertTrue(tests.contains("branches: [master"))
        XCTAssertFalse(tests.contains("branches: [main"))
        XCTAssertTrue(testFlight.contains("      - master"))
        XCTAssertFalse(testFlight.contains("      - main"))
    }

    func testCommentsDoNotClaimUnavailableTypedNMPAPI() throws {
        let adapter = repositoryRoot
            .appendingPathComponent("App/Sources/Services/NMPEpisodeCommentsRepository.swift")
        let commentsRepository = try source("App/Sources/Services/EpisodeCommentsRepository.swift")
        let commentsSection = try source(
            "App/Sources/Features/EpisodeDetail/EpisodeCommentsSection.swift"
        )
        let appMain = try source("App/Sources/AppMain.swift")

        XCTAssertFalse(FileManager.default.fileExists(atPath: adapter.path))
        XCTAssertFalse(commentsRepository.contains("POD0_NMP_TYPED_NIP22"))
        XCTAssertFalse(commentsRepository.contains("kind: 1111"))
        XCTAssertTrue(commentsRepository.contains("pablof7z/nmp#572"))
        XCTAssertTrue(commentsSection.contains("switch repository.availability"))
        XCTAssertFalse(appMain.contains(".episodeCommentsRepository"))
    }

    private func source(_ path: String) throws -> String {
        try String(contentsOf: repositoryRoot.appendingPathComponent(path), encoding: .utf8)
    }
}
