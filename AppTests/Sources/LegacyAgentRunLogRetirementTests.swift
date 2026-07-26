import Foundation
import XCTest
@testable import Podcastr

final class LegacyAgentRunLogRetirementTests: XCTestCase {
    func testRetiresExactLegacyRunLogAndIsIdempotent() throws {
        let root = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = try runLogURL(in: root)
        try Data("private legacy diagnostics".utf8).write(to: source)

        try LegacyAgentRunLogRetirement.run(fileURL: source)
        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))

        try LegacyAgentRunLogRetirement.run(fileURL: source)
        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))
    }

    func testRetirementPreservesUnknownSiblingFiles() throws {
        let root = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = try runLogURL(in: root)
        let sibling = source.deletingLastPathComponent()
            .appendingPathComponent("keep.txt")
        try Data("private legacy diagnostics".utf8).write(to: source)
        try Data("keep".utf8).write(to: sibling)

        try LegacyAgentRunLogRetirement.run(fileURL: source)

        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: sibling.path))
    }

    func testUnsafePathFailsClosedWithoutDeletingIt() throws {
        let root = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("runs.json")
        try Data("keep".utf8).write(to: source)

        XCTAssertThrowsError(try LegacyAgentRunLogRetirement.run(fileURL: source))
        XCTAssertTrue(FileManager.default.fileExists(atPath: source.path))
    }

    func testDirectoryMasqueradingAsRunLogFailsClosed() throws {
        let root = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = try runLogURL(in: root)
        try FileManager.default.removeItem(at: source)
        try FileManager.default.createDirectory(
            at: source,
            withIntermediateDirectories: false
        )
        let nested = source.appendingPathComponent("keep.txt")
        try Data("keep".utf8).write(to: nested)

        XCTAssertThrowsError(try LegacyAgentRunLogRetirement.run(fileURL: source))
        XCTAssertTrue(FileManager.default.fileExists(atPath: nested.path))
    }

    private func runLogURL(in root: URL) throws -> URL {
        let directory = root.appendingPathComponent("AgentRunLog", isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
        let source = directory.appendingPathComponent("runs.json")
        FileManager.default.createFile(atPath: source.path, contents: nil)
        return source
    }

    private func temporaryDirectory() -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try! FileManager.default.createDirectory(
            at: url,
            withIntermediateDirectories: true
        )
        return url
    }
}
