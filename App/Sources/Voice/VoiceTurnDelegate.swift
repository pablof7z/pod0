import Foundation

// MARK: - VoiceTurnDelegate

/// Bridge between `AudioConversationManager` (Voice) and the agent session
/// (currently `AgentChatSession` in `Features/Agent/`).
///
/// `AgentChatSession` is intentionally NOT modified by Lane 8 — Lane 10
/// (or the orchestrator at merge time) supplies a small adapter that
/// conforms `AgentChatSession` to this protocol. That adapter owns the
/// observation of `streamingContent` / `phase` and translates them into
/// the streaming-text contract Voice needs.
///
/// ## Why a streaming AsyncThrowingStream?
///
/// Voice mode wants three signals per turn:
///   1. Incremental assistant text — to feed the TTS client and captions
///      as soon as the first sentence is available (sub-second latency).
///   2. A clean "this turn finished" signal — so we transition out of the
///      `speaking` state and arm the recogniser for the next user utterance.
///   3. A failure signal — so the manager can transition into `error(_)`.
///
/// `AsyncThrowingStream<TurnEvent, Error>` carries all three with backpressure
/// for free. The alternative — observing `@Observable` properties on
/// `AgentChatSession` — leaks main-actor coupling into the manager and makes
/// barge-in cancellation racy. Streams are cancellable via `Task.cancel()`.
///
/// All methods are `@MainActor`-isolated because every conforming type so
/// far is a main-actor `@Observable` session. If a future implementation is
/// off-main, redeclare per-method isolation rather than dropping the
/// `@MainActor` here.
@MainActor
protocol VoiceTurnDelegate: AnyObject {

    /// Submit a finalised user utterance and return a stream of events for
    /// the agent's response. Implementations:
    ///   - Append the user message to the chat transcript exactly once.
    ///   - Yield `.partialText(_)` events as the assistant streams.
    ///   - Yield `.finalText(_)` once the turn produces its final text-only
    ///     reply (or an empty string if the turn ended in tool calls only).
    ///   - Finish the stream cleanly on success or throw on failure.
    ///
    /// The stream MUST be cancellable: when the user barges in, the manager
    /// cancels the consuming `Task` to unwind any in-flight LLM call.
    ///
    /// TODO(run-logs): The eventual `AgentChatSession` adapter MUST pass
    /// `source: .voiceMessage` when invoking `startSend(...)`, otherwise
    /// voice runs will be mis-tagged as `.typedChat` in Run History.
    func submitUtterance(_ text: String) -> AsyncThrowingStream<VoiceTurnEvent, Error>

    /// Whether the underlying agent session can accept a new utterance
    /// right now. False while a previous turn is still streaming.
    var canSubmit: Bool { get }
}

// MARK: - VoiceTurnEvent

/// Events emitted during one voice turn.
enum VoiceTurnEvent: Sendable, Equatable {
    /// Streaming partial assistant text. Cumulative — each event carries
    /// the full text so far, not just the delta. This matches how
    /// `AgentChatSession.streamingContent` is observed.
    case partialText(String)

    /// The final, complete assistant text for this turn. After this event
    /// the stream finishes normally.
    case finalText(String)

    /// The agent invoked a tool. Voice doesn't render tool results inline;
    /// it just acknowledges with a brief "running tools" note via the
    /// caption channel and returns to listening once the turn finishes.
    case toolInvocation(name: String)
}
