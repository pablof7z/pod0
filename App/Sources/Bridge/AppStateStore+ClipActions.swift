import Foundation

struct KernelQuoteResolution: Decodable, Equatable {
    let ok: Bool
    let startSecs: Double
    let endSecs: Double
    let transcriptText: String
    let speaker: String?
    let refinementStatus: String

    var speakerID: UUID? {
        speaker.flatMap(UUID.init(uuidString:))
    }

    enum CodingKeys: String, CodingKey {
        case ok
        case startSecs = "start_secs"
        case endSecs = "end_secs"
        case transcriptText = "transcript_text"
        case speaker
        case refinementStatus = "refinement_status"
    }
}

extension AppStateStore {
    @discardableResult
    func kernelCreateClip(
        id: UUID = UUID(),
        episodeID: UUID,
        startSecs: Double,
        endSecs: Double,
        title: String?,
        source: Clip.Source
    ) -> DispatchResult? {
        kernelCreateClip(
            id: id,
            episodeID: episodeID.uuidString,
            startSecs: startSecs,
            endSecs: endSecs,
            title: title,
            source: source
        )
    }

    @discardableResult
    func kernelCreateClip(
        id: UUID = UUID(),
        episodeID: String,
        startSecs: Double,
        endSecs: Double,
        title: String?,
        source: Clip.Source
    ) -> DispatchResult? {
        var body: [String: Any] = [
            "op": "create",
            "episode_id": episodeID,
            "start_secs": startSecs,
            "end_secs": endSecs,
            "source": source.rawValue,
            "client_clip_id": id.uuidString,
        ]
        if let title, !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            body["title"] = title
        }
        return kernel?.dispatch(namespace: "podcast.clip", body: body)
    }

    @discardableResult
    func kernelAutoSnip(episodeID: UUID, positionSecs: Double, source: Clip.Source) -> UUID? {
        let id = UUID()
        guard let result = kernel?.dispatch(namespace: "podcast.clip",
                                            body: ["op": "auto_snip",
                                                   "episode_id": episodeID.uuidString,
                                                   "position_secs": positionSecs,
                                                   "source": source.rawValue,
                                                   "client_clip_id": id.uuidString]) else {
            return nil
        }
        if case .failure = result {
            return nil
        }
        return id
    }

    func kernelDeleteClip(id: UUID) {
        kernel?.dispatch(namespace: "podcast.clip",
                         body: ["op": "delete",
                                "clip_id": id.uuidString])
    }

    func kernelResolveQuote(episodeID: UUID, positionSecs: Double) async -> KernelQuoteResolution? {
        guard let kernel else { return nil }
        let result = kernel.dispatchSilent(namespace: "podcast.clip",
                                           body: ["op": "resolve_quote",
                                                  "episode_id": episodeID.uuidString,
                                                  "position_secs": positionSecs])
        guard case let .accepted(correlationId) = result, !correlationId.isEmpty else {
            return nil
        }

        let registry = kernel.actionResultsRegistry
        let entry = try? await registry.awaitResult(correlationID: correlationId)
        guard let resultJSON = entry?.resultJSON,
              let data = resultJSON.data(using: String.Encoding.utf8),
              let decoded = try? JSONDecoder().decode(KernelQuoteResolution.self, from: data),
              decoded.ok
        else {
            return nil
        }
        return decoded
    }
}
