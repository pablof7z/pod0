import Foundation

// MARK: - Podcast action dispatch

extension AgentTools {
    static func actionTool(op: String, payload: [String: Any]) async -> String? {
        // Serialize the non-Sendable `[String: Any]` payload to a Sendable JSON
        // string synchronously (no `await` before this point), then hand off to
        // the string-based variant. Keeps the non-Sendable dict from being
        // `sending` across an actor boundary under Swift 6 concurrency.
        var request = payload
        request["op"] = op
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await actionToolJSON(json)
    }

    /// JSON-string variant of `actionTool`. Only `Sendable` values (a `String`
    /// and an `Int` bit pattern) cross actor boundaries here.
    static func actionToolJSON(_ json: String) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                guard let result = nmp_app_podcast_agent_action_tool(handle, ptr) else {
                    return nil
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }
}
