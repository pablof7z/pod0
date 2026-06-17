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
        let voices: [ElevenLabsVoice]
        do {
            voices = try await ElevenLabsVoicesService().fetchVoices()
        } catch ElevenLabsVoicesError.missingAPIKey {
            return toolError("ElevenLabs API key is not configured. Add it in Settings → AI.")
        } catch {
            return toolError("Could not fetch voices from ElevenLabs: \(error.localizedDescription)")
        }

        return await voiceListEnvelope(args: args, voices: voices)
            ?? toolError("Voice list shaping is unavailable")
    }

    private static func voiceListEnvelope(args: [String: Any], voices: [ElevenLabsVoice]) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits else { return nil }
        let request: [String: Any] = [
            "args": args,
            "voices": voices.map(rawVoiceRow),
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                guard let result = nmp_app_podcast_agent_voice_list(handle, ptr) else {
                    return nil
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }

    private static func rawVoiceRow(_ voice: ElevenLabsVoice) -> [String: Any] {
        var row: [String: Any] = [
            "voice_id": voice.voiceID,
            "name": voice.name,
            "category": voice.category,
            "labels": voice.labels,
        ]
        if let gender = voice.gender { row["gender"] = gender }
        if let accent = voice.accent { row["accent"] = accent }
        if let age = voice.age { row["age"] = age }
        if let useCase = voice.useCase { row["use_case"] = useCase }
        if let description = voice.descriptionLabel { row["description"] = description }
        if let preview = voice.previewURL { row["preview_url"] = preview.absoluteString }
        return row
    }
}
