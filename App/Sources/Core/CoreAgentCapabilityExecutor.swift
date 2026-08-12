import Pod0Core

@MainActor
protocol CoreAgentCapabilityExecuting: AnyObject {
    func execute(_ request: AgentCapabilityRequest) async -> AgentCapabilityOutcome
}

/// Executes exact Rust-authorized platform primitives. It does not choose the
/// active action, authorize it, change its arguments, or commit durable state.
@MainActor
final class LiveCoreAgentCapabilityExecutor: CoreAgentCapabilityExecuting {
    private let engine: AudioEngine
    private let playback: PlaybackState?
    private weak var store: AppStateStore?
    private let catalogSearch: PodcastCatalogEpisodeSearchService
    private let tts: any TTSClientProtocol
    private let generatedAudioStore: CoreAgentGeneratedAudioFileStore

    init(
        engine: AudioEngine,
        playback: PlaybackState? = nil,
        store: AppStateStore? = nil,
        catalogSearch: PodcastCatalogEpisodeSearchService = PodcastCatalogEpisodeSearchService(),
        tts: any TTSClientProtocol = ElevenLabsTTSClient(),
        generatedAudioStore: CoreAgentGeneratedAudioFileStore = CoreAgentGeneratedAudioFileStore()
    ) {
        self.engine = engine
        self.playback = playback
        self.store = store
        self.catalogSearch = catalogSearch
        self.tts = tts
        self.generatedAudioStore = generatedAudioStore
    }

    func execute(_ request: AgentCapabilityRequest) async -> AgentCapabilityOutcome {
        switch request.action {
        case .search(let tool, let query, let scope, let limit, let executeFirst)
            where tool == .searchPodcastDirectory:
            guard let store else {
                return .failed(safeDetail: "Podcast catalog is unavailable")
            }
            do {
                let result = try await catalogSearch.search(
                    episodeQuery: query,
                    podcastHint: scope,
                    limit: Int(limit),
                    store: store
                )
                if executeFirst, let episode = result.episodes.first {
                    guard let playback else {
                        return .failed(safeDetail: "Podcast playback is unavailable")
                    }
                    playback.enqueueSegments([.episode(episode.id)], playNow: true)
                }
                return .succeeded(boundedResult: result.boundedResult)
            } catch PodcastCatalogEpisodeSearchService.SearchError.noMatches {
                return .succeeded(boundedResult: #"{"episodes":[]}"#)
            } catch is CancellationError {
                return .cancelled
            } catch {
                return .failed(safeDetail: "The public podcast catalog could not be searched")
            }
        case .playEpisode(
            let episodeID,
            let startMilliseconds,
            let endMilliseconds,
            let placement
        ):
            guard let id = episodeID.uuid,
                  store?.episode(id: id) != nil,
                  let playback
            else {
                return .failed(safeDetail: "The requested episode is unavailable")
            }
            let item = QueueItem(
                episodeID: id,
                startSeconds: startMilliseconds.map { Double($0) / 1_000 },
                endSeconds: endMilliseconds.map { Double($0) / 1_000 }
            )
            switch placement {
            case .now:
                playback.enqueueSegments([item], playNow: true)
            case .next:
                playback.insertNext(item)
            case .back:
                playback.enqueueItem(item)
            case .unsupported:
                return .failed(safeDetail: "The requested queue position is unsupported")
            }
            return .succeeded(boundedResult: #"{"accepted":true}"#)
        case .noArguments(let tool) where tool == .pausePlayback:
            guard engine.episode != nil else {
                return .failed(safeDetail: "Playback media is unavailable")
            }
            engine.pause()
            return .succeeded(boundedResult: #"{"paused":true}"#)
        case .setPlaybackRate(let permille):
            guard engine.episode != nil else {
                return .failed(safeDetail: "Playback media is unavailable")
            }
            engine.setRate(Double(permille) / 1_000)
            return .succeeded(boundedResult: #"{"rate_permille":\#(permille)}"#)
        case .generateTtsEpisode(_, _, let script, let voiceID):
            guard let target = request.generatedAudioTarget else {
                return .failed(safeDetail: "Generated audio target is unavailable")
            }
            do {
                let evidence = try await generatedAudioStore.stage(
                    target: target,
                    mode: request.executionMode,
                    script: script,
                    voiceID: voiceID ?? ElevenLabsTTSClient.defaultVoiceID,
                    tts: tts
                )
                return .generatedAudioStaged(evidence: evidence)
            } catch CoreAgentGeneratedAudioFileStore.StoreError.missingRecoveryArtifact {
                return .outcomeAmbiguous
            } catch is CancellationError {
                return .cancelled
            } catch ElevenLabsTTSError.missingAPIKey {
                return .failed(safeDetail: "Text-to-speech is not configured")
            } catch {
                return .failed(safeDetail: "Generated audio could not be saved")
            }
        default:
            return .failed(safeDetail: "Native agent capability is unsupported")
        }
    }
}

@MainActor
final class UnavailableCoreAgentCapabilityExecutor: CoreAgentCapabilityExecuting {
    func execute(_ request: AgentCapabilityRequest) async -> AgentCapabilityOutcome {
        .failed(safeDetail: "Native agent capability is unavailable")
    }
}
