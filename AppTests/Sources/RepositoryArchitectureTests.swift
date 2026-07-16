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
    }

    func testRepositoryDependenciesAreSelfContainedAndPinned() throws {
        let project = try source("Project.swift")
        let revision = try source("Vendor/nmp-revision.txt")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let nmpConfiguration = try source("App/Sources/NMP/Pod0NMPConfiguration.swift")

        XCTAssertFalse(project.contains("../"))
        XCTAssertTrue(project.contains("https://github.com/pablof7z/ios-shake-feedback"))
        XCTAssertTrue(project.contains("requirement: .exact(\"1.0.0\")"))
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

    private func source(_ path: String) throws -> String {
        try String(contentsOf: repositoryRoot.appendingPathComponent(path), encoding: .utf8)
    }
}
