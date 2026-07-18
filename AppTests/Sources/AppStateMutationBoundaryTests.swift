import XCTest

final class AppStateMutationBoundaryTests: XCTestCase {
    func testProductionMutationGateStaysInsideStateDomain() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let sources = repositoryRoot.appendingPathComponent("App/Sources")
        let stateDirectory = sources.appendingPathComponent("State").standardizedFileURL.path
        let enumerator = try XCTUnwrap(
            FileManager.default.enumerator(
                at: sources,
                includingPropertiesForKeys: nil
            )
        )
        var violations: [String] = []
        for case let file as URL in enumerator where file.pathExtension == "swift" {
            guard !file.standardizedFileURL.path.hasPrefix(stateDirectory + "/") else {
                continue
            }
            let contents = try String(contentsOf: file, encoding: .utf8)
            if contents.contains(".mutateState") {
                violations.append(file.path.replacingOccurrences(
                    of: repositoryRoot.path + "/",
                    with: ""
                ))
            }
        }
        XCTAssertEqual(
            violations,
            [],
            "Production state mutations must use domain APIs: \(violations)"
        )
    }
}
