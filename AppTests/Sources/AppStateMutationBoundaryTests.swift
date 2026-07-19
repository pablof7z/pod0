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

    func testRustOwnedNotesHaveOneNativeProjectionAssignment() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let sources = repositoryRoot.appendingPathComponent("App/Sources")
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: sources,
            includingPropertiesForKeys: nil
        ))
        var projectionAssignments: [String] = []
        var directMutators: [String] = []
        for case let file as URL in enumerator where file.pathExtension == "swift" {
            let contents = try String(contentsOf: file, encoding: .utf8)
            let relative = file.path.replacingOccurrences(of: repositoryRoot.path + "/", with: "")
            for line in contents.split(separator: "\n") {
                let compact = line.filter { !$0.isWhitespace }
                let isAuthorityFlag = relative ==
                    "App/Sources/State/Persistence+SharedNotes.swift"
                    && compact.contains("sharedArtifactAuthority.withLock")
                let isMetadataCleanup = relative ==
                    "App/Sources/State/Persistence+SharedNotes.swift"
                    && compact.contains("metadata.notes=[]")
                if compact.contains(".notes=")
                    && !isAuthorityFlag
                    && !isMetadataCleanup {
                    projectionAssignments.append(relative)
                }
            }
            if Self.containsCollectionMutation(contents, property: "notes") {
                directMutators.append(relative)
            }
        }
        XCTAssertEqual(projectionAssignments, ["App/Sources/State/AppStateStore+SharedLibrary.swift"])
        XCTAssertEqual(directMutators, [])

        let notesAPI = try String(
            contentsOf: sources.appendingPathComponent("State/AppStateStore+Notes.swift"),
            encoding: .utf8
        )
        XCTAssertFalse(notesAPI.contains("mutateState"))
    }

    func testRustOwnedClipsHaveOneNativeProjectionAssignment() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let sources = repositoryRoot.appendingPathComponent("App/Sources")
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: sources,
            includingPropertiesForKeys: nil
        ))
        var projectionAssignments: [String] = []
        var directMutators: [String] = []
        for case let file as URL in enumerator where file.pathExtension == "swift" {
            let contents = try String(contentsOf: file, encoding: .utf8)
            let relative = file.path.replacingOccurrences(of: repositoryRoot.path + "/", with: "")
            for line in contents.split(separator: "\n") {
                let compact = line.filter { !$0.isWhitespace }
                let isAuthorityFlag = relative ==
                    "App/Sources/State/Persistence+SharedClips.swift"
                    && compact.contains("sharedArtifactAuthority.withLock")
                let isMetadataCleanup = relative ==
                    "App/Sources/State/Persistence+SharedNotes.swift"
                    && compact.contains("metadata.clips=[]")
                if compact.contains(".clips=")
                    && !isAuthorityFlag
                    && !isMetadataCleanup {
                    projectionAssignments.append(relative)
                }
            }
            if Self.containsCollectionMutation(contents, property: "clips") {
                directMutators.append(relative)
            }
        }
        XCTAssertEqual(projectionAssignments, ["App/Sources/State/AppStateStore+SharedLibrary.swift"])
        XCTAssertEqual(directMutators, [])

        let clipsAPI = try String(
            contentsOf: sources.appendingPathComponent("State/AppStateStore+Clips.swift"),
            encoding: .utf8
        )
        XCTAssertFalse(clipsAPI.contains("mutateState"))
    }

    private static func containsCollectionMutation(
        _ contents: String,
        property: String
    ) -> Bool {
        let base = ".\(property)"
        let mutators = [
            "append", "insert", "remove", "removeAll", "removeFirst",
            "removeLast", "popLast", "replaceSubrange", "sort", "reverse",
            "shuffle", "swapAt", "partition", "withUnsafeMutableBufferPointer"
        ]
        return contents.split(separator: "\n").contains { line in
            let compact = line.filter { !$0.isWhitespace }
            if compact.contains("\(base)+=") || compact.contains("\(base)-=") {
                return true
            }
            if compact.contains("\(base)[") && compact.contains("]=") {
                return true
            }
            return mutators.contains { mutation in
                compact.contains("\(base).\(mutation)(")
            }
        }
    }
}
