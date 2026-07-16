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

    func testRepositoryDependenciesAreSelfContainedAndPinned() throws {
        let project = try source("Project.swift")
        let revision = try source("Vendor/nmp-revision.txt")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let nmpConfiguration = try source("App/Sources/NMP/Pod0NMPConfiguration.swift")
        let shakeRevision = try source("Vendor/shake-feedback-revision.txt")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let shakeStager = try source("ci_scripts/stage_shake_feedback_package.sh")

        XCTAssertFalse(project.contains("../"))
        XCTAssertTrue(project.contains(".local(path: \"build/dependencies/ios-shake-feedback\")"))
        XCTAssertTrue(shakeStager.contains("SOURCE_URL=\"https://github.com/pablof7z/ios-shake-feedback\""))
        XCTAssertTrue(shakeStager.contains("SOURCE_VERSION=\"1.0.0\""))
        XCTAssertTrue(project.contains(".local(path: \"Vendor/nmp/Packages/NMP\")"))
        XCTAssertNotNil(revision.range(of: "^[0-9a-f]{40}$", options: .regularExpression))
        XCTAssertTrue(nmpConfiguration.contains("static let testedRevision = \"\(revision)\""))
        XCTAssertNotNil(shakeRevision.range(of: "^[0-9a-f]{40}$", options: .regularExpression))
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
