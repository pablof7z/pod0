import Foundation

// MARK: - TranscriptIngestService auto-ingest scheduling
//
// Split out from TranscriptIngestService.swift so the main file stays under
// AGENTS.md's 500-line hard cap. Swift supplies native file-availability facts,
// asks Rust which episodes are actionable, and schedules host-side ingest
// execution for the returned ids.

extension TranscriptIngestService {

    private struct TranscriptSourceLabelResponse: Decodable {
        let label: String?
        let error: String?
    }

    /// Ask Rust for the human-readable label used on the `transcript.ready`
    /// Diagnostics event. Swift only passes the raw source tag.
    static func sourceDisplayName(
        _ source: TranscriptState.Source,
        kernel: KernelModel?
    ) -> String? {
        guard let handle = kernel?.podcastHandlePointer else { return nil }
        let request: [String: Any] = [
            "op": "transcript_source_label",
            "source": source.rawValue,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        let envelope = json.withCString { ptr -> String? in
            guard let result = nmp_app_podcast_agent_action_tool(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            return String(cString: result)
        }
        guard let envelope,
              let responseData = envelope.data(using: .utf8),
              let response = try? JSONDecoder().decode(TranscriptSourceLabelResponse.self, from: responseData),
              response.error == nil
        else { return nil }
        return response.label
    }

    /// Ask Rust for the provider label attached to transcript attempt/failure
    /// Diagnostics. Swift only passes the raw provider tag.
    static func providerDisplayName(
        _ provider: STTProvider,
        kernel: KernelModel?
    ) -> String? {
        guard let handle = kernel?.podcastHandlePointer else { return nil }
        let request: [String: Any] = [
            "op": "stt_provider_label",
            "provider": provider.rawValue,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        let envelope = json.withCString { ptr -> String? in
            guard let result = nmp_app_podcast_agent_action_tool(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            return String(cString: result)
        }
        guard let envelope,
              let responseData = envelope.data(using: .utf8),
              let response = try? JSONDecoder().decode(TranscriptSourceLabelResponse.self, from: responseData),
              response.error == nil
        else { return nil }
        return response.label
    }

    /// Convenience: ask Rust for up to `maxCount` actionable episodes and
    /// execute host-side ingest for the returned ids. Useful as a background
    /// warmup once the user lands on the library.
    func ingestPending(maxCount: Int = 5) async {
        guard let appStore = appStore else {
            return
        }
        let localAudio = Dictionary(
            appStore.episodes.map { episode in
                (episode.id, EpisodeDownloadStore.shared.exists(for: episode))
            },
            uniquingKeysWith: { first, _ in first }
        )
        let candidateIDs = appStore.kernel?.transcriptAutoIngestCandidates(
            maxCount: maxCount,
            localAudioAvailable: localAudio
        ) ?? []
        for episodeID in candidateIDs {
            await ingest(episodeID: episodeID)
        }
    }

    /// Triggered from `AppStateStore.upsertEpisodes` whenever a feed refresh
    /// surfaces brand-new episode IDs. Filters to episodes the user would
    /// benefit from ingesting and dispatches an async `ingest` per candidate.
    ///
    /// Rust owns the inclusion rule, ordering, and cap: per-category opt-out,
    /// publisher auto-ingest toggle, AI fallback toggle, provider resolution,
    /// key presence, and local-file requirements. Swift schedules only the ids
    /// Rust returns.
    func evaluateAutoIngest(newEpisodeIDs: [UUID]) {
        guard !newEpisodeIDs.isEmpty else { return }
        guard let appStore = appStore else { return }
        let episodes = newEpisodeIDs.compactMap { appStore.episode(id: $0) }
        let localAudio = Dictionary(
            episodes.map { episode in
                (episode.id, EpisodeDownloadStore.shared.exists(for: episode))
            },
            uniquingKeysWith: { first, _ in first }
        )
        let candidates = appStore.kernel?.transcriptAutoIngestCandidates(
            maxCount: newEpisodeIDs.count,
            episodeIDs: newEpisodeIDs,
            localAudioAvailable: localAudio
        ) ?? []
        guard !candidates.isEmpty else { return }
        for episodeID in candidates {
            Task { @MainActor [weak self] in
                await self?.ingest(episodeID: episodeID)
            }
        }
    }
}
