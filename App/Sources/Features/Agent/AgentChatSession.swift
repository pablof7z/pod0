import Foundation
import Observation
import os.log

/// Live state for one agent chat. Holds the visible transcript, the raw
/// LLM-protocol message list, and the bookkeeping needed for retry/regenerate.
///
/// Multi-conversation support lives in `AgentChatSession+Conversations.swift`;
/// the turn loop (LLM streaming, tool dispatch, batching) lives in
/// `AgentChatSession+Turns.swift`. This file holds the class declaration,
/// init/auto-resume, and the small bits of state every other file consults.
@MainActor
@Observable
final class AgentChatSession {

    let logger = Logger.app("AgentChatSession")
    enum Phase: Equatable {
        case idle
        case sending
        case failed(String)
    }

    /// Auto-resume threshold. If the most recent conversation was updated
    /// within this window when the chat sheet opens, the user picks up where
    /// they left off; otherwise they get a fresh conversation. Picked at 3
    /// hours — the same listening session continues but next-day opens start
    /// clean.
    static let autoResumeWindow: TimeInterval = 3 * 60 * 60

    // MARK: - Visible state
    //
    // Several of these are `var` rather than `private(set)` because the
    // sibling extension files in the same module need to mutate them and
    // `private(set)` blocks cross-file writes. By convention, only methods on
    // `AgentChatSession` itself (this file + the two extension files) write
    // to them; the View layer reads only.

    var messages: [ChatMessage] = []
    var phase: Phase = .idle
    /// True only when the session was auto-resumed by the time-based heuristic.
    /// The view uses this to show the "Continuing from your previous session"
    /// banner; explicit user-driven loads (history sheet, new conversation)
    /// leave this `false` so the banner doesn't redundantly announce a choice
    /// the user just made.
    var loadedFromHistory: Bool = false
    var lastFailedMessage: String?
    /// ID of the conversation backing this session. Updated whenever the user
    /// starts a new chat or switches to one from the history sheet.
    private(set) var currentConversationID: UUID
    /// When `true`, every subsequent `streamCompletion` call uses
    /// `settings.agentThinkingModel` instead of `settings.agentInitialModel`.
    /// Flipped by the in-band `upgrade_thinking` tool call. Resets to `false`
    /// on every new conversation so cheap-model defaults always apply first.
    var isUpgraded: Bool = false
    /// Skill ids the agent has opted into for this conversation via the
    /// in-band `use_skill` tool. Each enabled skill contributes its tool
    /// schemas to the per-turn tool list. Resets to `[]` on every new
    /// conversation so the prompt stays lean by default.
    var enabledSkills: Set<String> = []
    /// Partial assistant content received so far during a streaming turn.
    /// Non-nil (including empty string) while a turn is actively streaming text.
    var streamingContent: String?
    /// The tool currently being dispatched, if any. Drives the typing indicator's
    /// status label.
    var currentToolName: String?

    /// When `true` the conversation is persisted with `isScheduledTask = true`
    /// so `ChatHistoryStore.mostRecent` skips it and the auto-resume path in
    /// the chat UI isn't hijacked by a background scheduled run.
    var isScheduledTask: Bool = false

    /// Composer prefill captured from the various pending-context bridges at
    /// init time. The view drains this exactly once via `consumeSeededDraft()`.
    private var seededDraft: String?
    private(set) var seededDraftShouldAutoSend: Bool = false

    // MARK: - Dependencies & internal bookkeeping

    let store: AppStateStore
    let history: ChatHistoryStore
    let podcastDeps: PodcastAgentToolDeps?
    /// Owner-consultation coordinator. Optional — when nil (tests,
    /// headless launches) the `ask` tool returns a typed error envelope
    /// rather than crashing. Held weakly because the coordinator's
    /// lifecycle is owned higher up (AppMain) and outlives any single
    /// chat session. Marked `@ObservationIgnored` because UI does not
    /// re-render on coordinator identity changes — only on the
    /// coordinator's own observable state (its `current` ask).
    @ObservationIgnored weak var askCoordinator: AgentAskCoordinator?
    @ObservationIgnored nonisolated(unsafe) private var delegationObserver: NSObjectProtocol?
    var rawMessages: [[String: Any]] = []
    var rawMessageCountAtLastSendStart: Int = 0
    var messageCountAtLastSendStart: Int = 0
    /// The currently-running send task. Held so `cancelSend()` can cancel it.
    var sendingTask: Task<Void, Never>?
    /// Title-generation task held so duplicate triggers don't pile up.
    var titleTask: Task<Void, Never>?

    let maxTurns: Int = 20

    init(
        store: AppStateStore,
        playback: PlaybackState? = nil,
        podcastDeps: PodcastAgentToolDeps? = nil,
        history: ChatHistoryStore = .shared,
        resumeWindow: TimeInterval? = nil,
        askCoordinator: AgentAskCoordinator? = nil,
        drainPendingContext: Bool = true
    ) {
        self.store = store
        self.history = history
        self.podcastDeps = podcastDeps ?? playback.map { LivePodcastAgentToolDeps.make(store: store, playback: $0) }
        self.askCoordinator = askCoordinator

        let window = resumeWindow ?? Self.autoResumeWindow
        if let recent = history.mostRecent,
           !recent.messages.isEmpty,
           Date().timeIntervalSince(recent.updatedAt) < window {
            self.currentConversationID = recent.id
            self.messages = recent.messages
            self.isUpgraded = recent.isUpgraded
            self.enabledSkills = recent.enabledSkills
            self.loadedFromHistory = true
        } else {
            self.currentConversationID = UUID()
        }

        if drainPendingContext {
            checkAndDrainPendingContext()
        }

        delegationObserver = NotificationCenter.default.addObserver(
            forName: .agentDelegationDidComplete,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            let id = notification.object as? UUID
            MainActor.assumeIsolated {
                guard let self, let id, id == self.currentConversationID else { return }
                if let convo = self.history.conversation(id: id) {
                    self.messages = convo.messages
                }
            }
        }
    }

    deinit {
        if let observer = delegationObserver {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    /// Checks the store for pending ask-agent context (voice note, chapter,
    /// transcript) and drains it into `seededDraft`. Safe to call multiple
    /// times — the store fields are cleared on first drain, so later calls
    /// are no-ops. Called from `init` and from `AgentChatView.onAppear` so
    /// that a persistent session picks up new context on each sheet open.
    func checkAndDrainPendingContext() {
        if let voiceNote = store.pendingVoiceNoteAgentContext {
            seededDraft = voiceNote.prefilledDraft
            seededDraftShouldAutoSend = true
            store.pendingVoiceNoteAgentContext = nil
            store.pendingChapterAgentContext = nil
            store.pendingTranscriptAgentContext = nil
        } else if let chapter = store.pendingChapterAgentContext {
            seededDraft = chapter.prefilledDraft
            store.pendingChapterAgentContext = nil
            store.pendingTranscriptAgentContext = nil
        } else if let pending = store.pendingTranscriptAgentContext {
            seededDraft = pending.prefilledDraft
            store.pendingTranscriptAgentContext = nil
        }
    }

    /// Returns the prefilled draft once and clears it. View calls this from
    /// `.onAppear` after wiring the session.
    func consumeSeededDraft() -> String? {
        let value = seededDraft
        seededDraft = nil
        return value
    }

    /// Returns the prefilled draft and the auto-send flag together, then
    /// clears both.
    func consumeSeededDraftWithAutoSend() -> (draft: String, autoSend: Bool)? {
        guard let value = seededDraft else { return nil }
        let shouldAutoSend = seededDraftShouldAutoSend
        seededDraft = nil
        seededDraftShouldAutoSend = false
        return (value, shouldAutoSend)
    }

    var canSend: Bool {
        if case .sending = phase { return false }
        return true
    }

    /// Cancels an in-flight streaming turn, discarding any partial content.
    func cancelSend() {
        sendingTask?.cancel()
        sendingTask = nil
    }

    /// Sets the current conversation ID. Internal helper for the +Conversations
    /// extension so it doesn't need to widen access on `currentConversationID`.
    func setCurrentConversationID(_ id: UUID) {
        currentConversationID = id
    }
}
