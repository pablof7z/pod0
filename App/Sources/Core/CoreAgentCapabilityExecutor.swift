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
    private let tts: any TTSClientProtocol
    private let generatedAudioStore: CoreAgentGeneratedAudioFileStore

    init(
        engine: AudioEngine,
        tts: any TTSClientProtocol = ElevenLabsTTSClient(),
        generatedAudioStore: CoreAgentGeneratedAudioFileStore = CoreAgentGeneratedAudioFileStore()
    ) {
        self.engine = engine
        self.tts = tts
        self.generatedAudioStore = generatedAudioStore
    }

    func execute(_ request: AgentCapabilityRequest) async -> AgentCapabilityOutcome {
        switch request.action {
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
