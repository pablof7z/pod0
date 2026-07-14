import Foundation
import Observation
import os.log

// MARK: - AudioConversationManager

/// Top-level voice conversation orchestrator. Owns the state machine and
/// wires together STT, TTS, the barge-in detector, the audio session
/// coordinator, and the agent turn delegate.
///
/// **Lifecycle**: instantiate once per app session (the View tab holds a
/// `@State` reference). Call `enterAmbientMode()` for hands-free operation
/// or `startPushToTalk()` / `endPushToTalk()` for press-to-talk UX.
///
/// **Cancellation discipline**: every state transition cancels the previous
/// stage's `Task`. This is what makes barge-in feel snappy — `interruptCurrentSpeech()`
/// just cancels the speaking task, and the cancellation propagates into the
/// TTS stream and stops audio output within ~1 frame.
@MainActor
@Observable
final class AudioConversationManager {

    private let logger = Logger.app("AudioConversationManager")

    // MARK: - Observed state

    private(set) var state: AudioConversationState = .idle
    let captions: VoiceCaptionLog = .init()

    /// Last partial transcription from the recogniser. UI binds this to
    /// the user-side caption row while listening.
    private(set) var liveUserUtterance: String = ""

    /// Last partial assistant text from the agent. UI binds this to the
    /// agent-side caption row while speaking.
    private(set) var liveAgentText: String = ""

    // MARK: - Collaborators

    private let stt: SpeechRecognizerServiceProtocol
    private let tts: TTSClientProtocol
    private let avFallback: AVSpeechFallback
    private let barge: BargeInDetectorProtocol
    private(set) var audioCoordinator: AudioSessionCoordinatorProtocol
    private weak var turnDelegate: VoiceTurnDelegate?

    /// Selected ElevenLabs voice ID. Defaults to the Flash default; user
    /// preference wires through Settings (Lane 10 / orchestrator).
    var voiceID: String = ElevenLabsTTSClient.defaultVoiceID

    /// True when in continuous ambient (hands-free) mode.
    private(set) var isAmbient: Bool = false

    /// True between `.optimisticPreview` and `.confirmed` (or the speaking
    /// task ending). The orb keys its rim-light + tint snap on this so the
    /// magic of the barge-in moment is visible even before the manager
    /// formally transitions to listening.
    private(set) var isUserBargingIn: Bool = false

    // MARK: - Internal task handles

    private var listeningTask: Task<Void, Never>?
    private var thinkingTask: Task<Void, Never>?
    private var speakingTask: Task<Void, Never>?
    private var bargeTask: Task<Void, Never>?

    /// Caption ID we're currently updating from the live recogniser.
    private var liveUserCaptionID: UUID?
    /// Caption ID we're updating from the live agent stream.
    private var liveAgentCaptionID: UUID?

    // MARK: - Init

    init(
        stt: SpeechRecognizerServiceProtocol = SpeechRecognizerService(),
        tts: TTSClientProtocol = ElevenLabsTTSClient(),
        avFallback: AVSpeechFallback = AVSpeechFallback(),
        barge: BargeInDetectorProtocol = BargeInDetector(),
        audioCoordinator: AudioSessionCoordinatorProtocol = NoopAudioSessionCoordinator(),
        turnDelegate: VoiceTurnDelegate? = nil
    ) {
        self.stt = stt
        self.tts = tts
        self.avFallback = avFallback
        self.barge = barge
        self.audioCoordinator = audioCoordinator
        self.turnDelegate = turnDelegate
    }

    // MARK: - Public configuration

    /// Inject the integration adapter. The Voice tab calls this after the
    /// app has constructed `AgentChatSession` so the manager can submit
    /// utterances. If left `nil` the manager uses `StubVoiceTurnDelegate`
    /// implicitly via `effectiveDelegate()`.
    func setTurnDelegate(_ delegate: VoiceTurnDelegate) {
        self.turnDelegate = delegate
    }

    func setAudioCoordinator(_ coordinator: AudioSessionCoordinatorProtocol) {
        self.audioCoordinator = coordinator
    }

    // MARK: - Public — push-to-talk

    /// Begin listening. Called when the user presses-and-holds the talk
    /// button. Idempotent — no-op when already listening.
    func startPushToTalk() {
        guard state == .idle || state == .speaking else {
            logger.debug("startPushToTalk ignored — state=\(String(describing: self.state), privacy: .public)")
            return
        }
        // Pressing PTT while speaking implicitly interrupts.
        if state == .speaking { interruptCurrentSpeech() }
        beginListening(ambient: false)
    }

    /// Finalise the current STT segment and submit to the agent.
    func endPushToTalk() {
        guard state == .listening else { return }
        // Tell the recogniser we've stopped capturing — it will fire one
        // last `.final` event which `runListeningLoop` picks up and submits.
        stt.stop()
    }

    // MARK: - Public — ambient mode

    /// Hands-free continuous conversation. The recogniser runs persistently
    /// and submits utterances on natural pauses. Barge-in is also armed
    /// during agent speech so the user can interrupt at any time.
    func enterAmbientMode() {
        guard !isAmbient else { return }
        isAmbient = true
        beginListening(ambient: true)
    }

    func exitAmbientMode() {
        guard isAmbient else { return }
        isAmbient = false
        cancelAll()
        Task { await audioCoordinator.endVoiceSession() }
        state = .idle
    }

    // MARK: - Public — interrupt

    /// Cancel any in-flight TTS and (in ambient mode) start listening again.
    /// In PTT mode we transition back to `idle` and wait for the next press.
    func interruptCurrentSpeech() {
        guard state == .speaking else { return }
        logger.info("Speech interrupted")
        speakingTask?.cancel()
        speakingTask = nil
        bargeTask?.cancel()
        bargeTask = nil
        isUserBargingIn = false
        avFallback.stopSpeaking()
        if isAmbient {
            beginListening(ambient: true)
        } else {
            state = .idle
        }
    }

    // MARK: - Public — briefing handoff

    /// Lane 9 (Briefings) calls this to take ownership of the audio output
    /// for the duration of a briefing. We tear down our own TTS, ask the
    /// audio coordinator to duck other media, and park in
    /// `duckedWhileBriefing` until the briefing finishes.
    ///
    /// `briefing` is opaque to us — Lane 9 owns the playback. We just need
    /// a handle so we can resume listening when it completes.
    func attachToBriefing(_ briefing: VoiceBriefingHandle) {
        speakingTask?.cancel()
        speakingTask = nil
        Task { @MainActor in
            do {
                try await audioCoordinator.duckOthersForBriefing()
                state = .duckedWhileBriefing
                await briefing.waitUntilFinished()
                try await audioCoordinator.unduckOthersAfterBriefing()
                if isAmbient {
                    beginListening(ambient: true)
                } else {
                    state = .idle
                }
            } catch {
                state = .error(VoiceError(from: error))
            }
        }
    }

    // MARK: - State machine internals

    private func beginListening(ambient: Bool) {
        cancelListeningOnly()
        liveUserUtterance = ""
        liveUserCaptionID = nil
        state = .listening
        listeningTask = Task { [weak self] in
            guard let self else { return }
            await self.runListeningLoop(ambient: ambient)
        }
    }

    private func runListeningLoop(ambient: Bool) async {
        do {
            try await audioCoordinator.beginVoiceCapture()
        } catch {
            state = .error(VoiceError(from: error))
            return
        }

        let stream = stt.startStreaming()
        do {
            for try await event in stream {
                if Task.isCancelled { return }
                switch event {
                case .partial(let text):
                    self.handlePartialUserText(text)
                case .final(let text):
                    self.handleFinalUserText(text)
                    await self.beginThinking(utterance: text, ambient: ambient)
                    return
                }
            }
        } catch {
            state = .error(VoiceError(from: error))
        }
    }

    private func handlePartialUserText(_ text: String) {
        liveUserUtterance = text
        if let id = liveUserCaptionID {
            captions.update(id: id, text: text, stability: .partial)
        } else {
            liveUserCaptionID = captions.appendPartial(.user, text: text)
        }
    }

    private func handleFinalUserText(_ text: String) {
        liveUserUtterance = text
        if let id = liveUserCaptionID {
            captions.finalize(id: id, text: text)
        } else {
            captions.appendFinal(.user, text: text)
        }
        liveUserCaptionID = nil
    }

    // MARK: - Thinking

    private func beginThinking(utterance: String, ambient: Bool) async {
        let trimmed = utterance.trimmed
        guard !trimmed.isEmpty else {
            // Empty utterance — fall back to listening or idle.
            if ambient {
                beginListening(ambient: true)
            } else {
                state = .idle
            }
            return
        }
        state = .thinking
        liveAgentText = ""
        liveAgentCaptionID = nil

        let delegate = effectiveDelegate()
        let stream = delegate.submitUtterance(trimmed)

        var finalText: String?
        do {
            for try await event in stream {
                if Task.isCancelled { return }
                switch event {
                case .partialText(let partial):
                    self.liveAgentText = partial
                    if let id = liveAgentCaptionID {
                        captions.update(id: id, text: partial, stability: .partial)
                    } else {
                        liveAgentCaptionID = captions.appendPartial(.agent, text: partial)
                    }
                case .finalText(let text):
                    finalText = text
                    if let id = liveAgentCaptionID {
                        captions.finalize(id: id, text: text)
                    } else {
                        captions.appendFinal(.agent, text: text)
                    }
                case .toolInvocation(let name):
                    self.logger.debug("Agent invoked tool: \(name, privacy: .public)")
                    captions.appendFinal(.agent, text: "Running \(name)…")
                }
            }
        } catch is CancellationError {
            return
        } catch {
            state = .error(VoiceError.agentFailed(error.localizedDescription))
            return
        }

        guard let final = finalText, !final.isBlank else {
            // Tool-only turn or empty reply — go back to listening / idle.
            if ambient {
                beginListening(ambient: true)
            } else {
                state = .idle
            }
            return
        }
        await beginSpeaking(text: final, ambient: ambient)
    }

    // MARK: - Speaking

    private func beginSpeaking(text: String, ambient: Bool) async {
        state = .speaking
        do {
            try await audioCoordinator.beginVoicePlayback()
        } catch {
            state = .error(VoiceError(from: error))
            return
        }

        // In ambient mode arm the barge-in detector concurrently with TTS
        // so the user can interrupt mid-sentence.
        if ambient {
            armBargeIn()
        }

        let client: TTSClientProtocol = tts.isConfigured ? tts : avFallback
        let stream = client.synthesizeStream(text: text, voiceID: voiceID)

        speakingTask = Task { [weak self] in
            guard let self else { return }
            do {
                for try await chunk in stream {
                    if Task.isCancelled { return }
                    self.barge.recordTTSOutput(chunk)
                    // Note: actual audio playback would route this chunk to
                    // an `AVAudioPlayerNode` owned by the Audio lane. The
                    // protocol contract with Lane 1 is that
                    // `beginVoicePlayback()` has prepared the route; we
                    // forward bytes via the (future) `audioCoordinator.play(chunk)`
                    // API. AVSpeechFallback handles its own playback so its
                    // chunks are sentinel-only.
                }
            } catch is CancellationError {
                return
            } catch {
                self.state = .error(VoiceError(from: error))
                return
            }
            // TTS finished naturally.
            if Task.isCancelled { return }
            self.bargeTask?.cancel()
            self.bargeTask = nil
            if ambient {
                self.beginListening(ambient: true)
            } else {
                self.state = .idle
            }
        }
    }

    private func armBargeIn() {
        bargeTask?.cancel()
        isUserBargingIn = false
        let stream = barge.start()
        bargeTask = Task { [weak self] in
            for await event in stream {
                guard let self else { return }
                if Task.isCancelled { return }
                self.logger.info("Barge-in event: \(String(describing: event), privacy: .public)")
                switch event {
                case .optimisticPreview:
                    // Surface the rim-light state without halting TTS — if
                    // the confirm never arrives the speaking task continues
                    // and the orb returns to its breath rhythm. This is the
                    // 'magic' window from the UX spec §5.
                    self.isUserBargingIn = true
                case .confirmed:
                    self.isUserBargingIn = false
                    self.interruptCurrentSpeech()
                    return
                }
            }
        }
    }

    // MARK: - Helpers

    private func effectiveDelegate() -> VoiceTurnDelegate {
        if let turnDelegate { return turnDelegate }
        // Stash a stub on first use so subsequent calls reuse it.
        let stub = StubVoiceTurnDelegate()
        self.turnDelegate = stub
        return stub
    }

    private func cancelAll() {
        listeningTask?.cancel()
        thinkingTask?.cancel()
        speakingTask?.cancel()
        bargeTask?.cancel()
        listeningTask = nil
        thinkingTask = nil
        speakingTask = nil
        bargeTask = nil
        isUserBargingIn = false
        stt.stop()
        barge.stop()
    }

    private func cancelListeningOnly() {
        listeningTask?.cancel()
        listeningTask = nil
        stt.stop()
    }
}
