import Pod0Core

@MainActor
protocol CoreAgentCapabilityExecuting: AnyObject {
    func execute(_ action: AgentToolAction) async -> AgentCapabilityOutcome
}

/// Executes exact Rust-authorized platform primitives. It does not choose the
/// active action, authorize it, change its arguments, or commit durable state.
@MainActor
final class LiveCoreAgentCapabilityExecutor: CoreAgentCapabilityExecuting {
    private let engine: AudioEngine

    init(engine: AudioEngine) {
        self.engine = engine
    }

    func execute(_ action: AgentToolAction) async -> AgentCapabilityOutcome {
        guard engine.episode != nil else {
            return .failed(safeDetail: "Playback media is unavailable")
        }
        switch action {
        case .noArguments(let tool) where tool == .pausePlayback:
            engine.pause()
            return .succeeded(boundedResult: #"{"paused":true}"#)
        case .setPlaybackRate(let permille):
            engine.setRate(Double(permille) / 1_000)
            return .succeeded(boundedResult: #"{"rate_permille":\#(permille)}"#)
        default:
            return .failed(safeDetail: "Native agent capability is unsupported")
        }
    }
}

@MainActor
final class UnavailableCoreAgentCapabilityExecutor: CoreAgentCapabilityExecuting {
    func execute(_ action: AgentToolAction) async -> AgentCapabilityOutcome {
        .failed(safeDetail: "Native agent capability is unavailable")
    }
}
