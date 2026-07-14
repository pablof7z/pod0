import Foundation

// MARK: - LiveEpisodeSummarizerAdapter
//
// Generates an on-demand episode summary by feeding the persisted transcript
// (when present) into a single OpenRouter chat call. When no transcript has
// been ingested or the user hasn't added an OpenRouter key yet, falls back to
// the publisher's `description` so the tool still returns something useful
// rather than an error envelope.
//
// Prompt layout uses `WikiOpenRouterClient.live` because that path already
// forces structured JSON output for OpenRouter and Ollama — we want a
// `{summary, bullets}` payload back, not a free-form chat reply.

struct LiveEpisodeSummarizerAdapter: EpisodeSummarizerProtocol {

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func summarizeEpisode(episodeID: EpisodeID, length: String?) async throws -> EpisodeSummary {
        guard let uuid = UUID(uuidString: episodeID),
              let episode = await store?.episode(id: uuid) else {
            return EpisodeSummary(episodeID: episodeID, summary: "", source: .unavailable)
        }
        let body = await Self.episodeBodyText(uuid: uuid, fallback: episode.description)
        guard !body.isEmpty else {
            return EpisodeSummary(
                episodeID: episodeID,
                summary: episode.description,
                source: .publisherDescription
            )
        }
        let model = await MainActor.run {
            store?.state.settings.memoryCompilationModel ?? Settings().memoryCompilationModel
        }
        let reference = LLMModelReference(storedID: model)
        guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            return EpisodeSummary(
                episodeID: episodeID,
                summary: episode.description,
                source: .publisherDescription
            )
        }
        let client = WikiOpenRouterClient.live(model: model)
        let lengthHint = (length ?? "medium").lowercased()
        let json = try await client.compile(
            systemPrompt: Self.systemPrompt(),
            userPrompt: Self.userPrompt(title: episode.title, length: lengthHint, body: body),
            feature: CostFeature.episodeSummary
        )
        return Self.parseSummary(episodeID: episodeID, json: json) ??
            EpisodeSummary(
                episodeID: episodeID,
                summary: episode.description,
                source: .publisherDescription
            )
    }

    // MARK: Helpers

    /// Pulls the full episode body either from the parsed transcript text or,
    /// when no transcript exists, from the publisher's show notes. Capped at
    /// 16k chars to keep the prompt budget sane.
    static func episodeBodyText(uuid: UUID, fallback: String) async -> String {
        if let transcript = TranscriptStore.shared.load(episodeID: uuid) {
            let joined = transcript.segments.map(\.text).joined(separator: " ")
            return String(joined.prefix(16_000))
        }
        return String(fallback.prefix(16_000))
    }

    static func systemPrompt() -> String {
        """
        You summarise podcast episodes for a busy listener. Always respond in JSON
        with the shape {"summary": String, "bullets": [String]}. Keep bullets under
        12 words each. Do not invent facts not present in the supplied content.
        """
    }

    static func userPrompt(title: String, length: String, body: String) -> String {
        let bulletCount = length == "short" ? 3 : (length == "long" ? 7 : 5)
        return """
        Episode title: \(title)
        Desired length: \(length) (\(bulletCount) bullets)
        Episode content (transcript or notes):
        \(body)
        """
    }

    static func parseSummary(episodeID: EpisodeID, json: String) -> EpisodeSummary? {
        guard let data = json.data(using: .utf8),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        let summary = (root["summary"] as? String) ?? ""
        let bullets = (root["bullets"] as? [String]) ?? []
        return EpisodeSummary(episodeID: episodeID, summary: summary, bulletPoints: bullets, source: .llm)
    }
}
