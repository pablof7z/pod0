import Foundation

// MARK: - list_available_voices
//
// Skill-gated tool unlocked by the `podcast_generation` skill. Fetches the
// user's ElevenLabs voice library so the agent can pick a `voice_id` that
// actually exists in the user's account. Voices are account-specific —
// without this, the agent would have to guess from the documented defaults.

extension AgentTools {

    static let listAvailableVoicesDefaultLimit = 30
    static let listAvailableVoicesMaxLimit = 50

    static func listAvailableVoicesTool(args: [String: Any]) async -> String {
        let apiKey: String
        do {
            guard let key = try ElevenLabsCredentialStore.apiKey(), !key.isEmpty else {
                return toolError("ElevenLabs API key is not configured. Add it in Settings → AI.")
            }
            apiKey = key
        } catch {
            return toolError("Could not read ElevenLabs API key: \(error.localizedDescription)")
        }

        let query = (args["query"] as? String)?.trimmed.nilIfEmpty?.lowercased()
        let limit = clampedLimit(
            args["limit"],
            default: listAvailableVoicesDefaultLimit,
            max: listAvailableVoicesMaxLimit
        )

        let voices: [ElevenLabsVoice]
        do {
            voices = try await ElevenLabsVoicesService().fetchVoices(apiKey: apiKey)
        } catch {
            return toolError("Could not fetch voices from ElevenLabs: \(error.localizedDescription)")
        }

        let filtered: [ElevenLabsVoice]
        if let q = query {
            filtered = voices.filter { $0.searchText.contains(q) }
        } else {
            filtered = voices
        }

        let trimmed = Array(filtered.prefix(limit))
        let rows: [[String: Any]] = trimmed.map { v in
            var row: [String: Any] = [
                "voice_id": v.voiceID,
                "name": v.name,
                "category": v.category,
            ]
            if let g = v.gender, !g.isEmpty { row["gender"] = g }
            if let a = v.accent, !a.isEmpty { row["accent"] = a }
            if let age = v.age, !age.isEmpty { row["age"] = age }
            if let u = v.useCase, !u.isEmpty { row["use_case"] = u }
            if let d = v.descriptionLabel, !d.isEmpty { row["description"] = d }
            if let preview = v.previewURL { row["preview_url"] = preview.absoluteString }
            return row
        }

        return toolSuccess([
            "total_available": voices.count,
            "total_matched": filtered.count,
            "results": rows,
        ])
    }
}
