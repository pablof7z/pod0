import Foundation

enum LegacyAgentRunLogRetirementError: Error, Equatable {
    case verificationFailed
}

enum LegacyAgentRunLogRetirement {
    static func run(
        fileURL: URL,
        fileManager: FileManager = .default
    ) throws {
        let source = fileURL.standardizedFileURL
        guard source.lastPathComponent == "runs.json",
              source.deletingLastPathComponent().lastPathComponent == "AgentRunLog"
        else {
            throw LegacyAgentRunLogRetirementError.verificationFailed
        }

        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(
            atPath: source.path,
            isDirectory: &isDirectory
        ) else {
            return
        }
        guard !isDirectory.boolValue else {
            throw LegacyAgentRunLogRetirementError.verificationFailed
        }

        do {
            try fileManager.removeItem(at: source)
        } catch {
            throw LegacyAgentRunLogRetirementError.verificationFailed
        }
        guard !fileManager.fileExists(atPath: source.path) else {
            throw LegacyAgentRunLogRetirementError.verificationFailed
        }

        let directory = source.deletingLastPathComponent()
        guard let entries = try? fileManager.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: nil
        ), entries.isEmpty else {
            return
        }
        try? fileManager.removeItem(at: directory)
    }
}
