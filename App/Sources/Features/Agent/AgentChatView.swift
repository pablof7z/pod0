import SwiftUI

/// Full-screen chat interface for the AI agent, presented as a sheet.
struct AgentChatView: View {

    // MARK: - Layout

    private enum Layout {
        static let timeSeparatorThreshold: TimeInterval = 15 * 60
        static let jumpButtonSize: CGFloat = 30
        static let bannerCloseIconSize: CGFloat = 11
        static let bannerCloseFrameSize: CGFloat = 22
        static let bannerIconSize: CGFloat = 14
        static let inputFieldPaddingH: CGFloat = 14
        static let inputFieldPaddingV: CGFloat = 10
        static let inputFieldCornerRadius: CGFloat = 22
        static let sendButtonSize: CGFloat = 38
        static let sendButtonIconSize: CGFloat = 17
        static let chipCornerRadius: CGFloat = 14
        static let bannerCornerRadius: CGFloat = 12
        static let welcomeIconSize: CGFloat = 44
        static let disconnectedIconSize: CGFloat = 40
        /// Character count appears below the field above this threshold.
        static let charCountThreshold: Int = 200
    }

    private struct IdentifiedBatch: Identifiable {
        let id: UUID
    }

    // `IdentifiedURL` was used to drive `.sheet(item:)` for the transcript
    // share — replaced by `SystemShareSheet.present(items:)` because
    // wrapping `UIActivityViewController` in a SwiftUI sheet renders blank
    // for file-URL items. See `Design/ShareSheet.swift` docstring for the
    // file-vs-string distinction.

    // MARK: - State

    @Environment(AppStateStore.self) private var store

    /// The session is owned by `RootView` (or another persistent owner) and
    /// passed in. This keeps the session — and any in-flight LLM task — alive
    /// across sheet dismissals.
    let session: AgentChatSession

    @State private var draft: String = ""
    @State private var presentedBatch: UUID?
    @State private var showSettingsHint = false
    @State private var bannerDismissed = false
    @State private var didSendInSession = false
    @State private var showHistory = false
    @State private var scrolledMessageID: AnyHashable? = nil
    // `transcriptURL` is no longer held as state — the export handler
    // presents the share sheet imperatively via `SystemShareSheet`.
    @FocusState private var inputFocused: Bool

    var body: some View {
        ZStack {
            background.ignoresSafeArea()
            content
        }
        .navigationTitle("Agent")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { toolbarItems }
        .onAppear {
            let reference = LLMModelReference(storedID: store.state.settings.agentInitialModel)
            let hasKey = LLMProviderCredentialResolver.hasAPIKey(for: reference.provider)
            showSettingsHint = !hasKey
            // Drain any pending ask-agent context that was set since the
            // session was last opened (e.g. long-press → Ask Agent while
            // the session was already alive but the sheet was closed).
            session.checkAndDrainPendingContext()
            // Long-press → ask-agent prefill. Drained once per sheet open so
            // a re-open without new context starts blank. Focus the input
            // so the user can append a question and Send without a tap.
            if let seeded = session.consumeSeededDraftWithAutoSend(), !seeded.draft.isEmpty {
                if seeded.autoSend {
                    session.startSend(seeded.draft, source: .typedChat)
                } else {
                    draft = seeded.draft
                    inputFocused = true
                }
            } else {
                inputFocused = hasKey
            }
        }
        .onChange(of: session.phase) { _, newPhase in
            switch newPhase {
            case .idle:
                if didSendInSession { Haptics.success() }
            case .failed:
                Haptics.error()
            case .sending:
                break
            }
        }
        .sheet(item: Binding(
            get: { presentedBatch.map(IdentifiedBatch.init) },
            set: { presentedBatch = $0?.id }
        )) { batch in
            AgentActivitySheet(batchID: batch.id)
        }
        .sheet(isPresented: $showHistory) {
            AgentChatHistoryView(
                history: session.history,
                currentID: session.currentConversationID,
                onSelect: { convo in
                    Task {
                        await session.switchToConversation(convo.id)
                        // A fresh switch is a fresh sheet — reset transient
                        // banners and bookkeeping so the loaded thread acts
                        // like the chat the user picked.
                        bannerDismissed = false
                        didSendInSession = false
                        draft = ""
                    }
                },
                onNew: {
                    Task {
                        await session.startNewConversation()
                        bannerDismissed = false
                        didSendInSession = false
                        draft = ""
                    }
                }
            )
        }
    }

    @ToolbarContentBuilder
    private var toolbarItems: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button {
                Haptics.selection()
                showHistory = true
            } label: {
                Image(systemName: "clock.arrow.circlepath")
            }
            .accessibilityLabel("Conversation history")
        }
        ToolbarItem(placement: .primaryAction) {
            Button {
                Haptics.selection()
                Task {
                    await session.startNewConversation()
                    bannerDismissed = false
                    didSendInSession = false
                    draft = ""
                }
            } label: {
                Image(systemName: "plus")
            }
            .accessibilityLabel("New conversation")
        }
        if !session.messages.isEmpty {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    exportTranscript(messages: session.messages)
                } label: {
                    Image(systemName: "square.and.arrow.up")
                }
                .accessibilityLabel("Export transcript")
            }
        }
    }

    private func exportTranscript(messages: [ChatMessage]) {
        var batchSummaries: [UUID: [String]] = [:]
        for msg in messages {
            if case .toolBatch(let batchID, _) = msg.role {
                let entries = store.agentActivity(forBatch: batchID)
                batchSummaries[batchID] = entries.map(\.summary)
            }
        }
        guard let url = AgentChatTranscriptExport.write(messages, batchSummaries: batchSummaries) else { return }
        Haptics.success()
        // Imperative present — SwiftUI's `.sheet { ShareSheet(...) }` wrapper
        // renders the activity controller as a blank sheet on iOS 16+ for
        // file-URL items.
        SystemShareSheet.present(items: [url])
    }

    @ViewBuilder
    private var content: some View {
        VStack(spacing: 0) {
            if shouldShowResumeBanner(session: session) {
                resumeBanner
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
            if session.messages.isEmpty {
                emptyState
            } else {
                transcript(session: session)
            }
            composer(session: session)
        }
        .animation(AppTheme.Animation.spring, value: shouldShowResumeBanner(session: session))
    }

    private func shouldShowResumeBanner(session: AgentChatSession) -> Bool {
        session.loadedFromHistory && !bannerDismissed && !didSendInSession
    }

    private var resumeBanner: some View {
        AgentChatResumeBanner(isDismissed: $bannerDismissed)
    }

    private func transcript(session: AgentChatSession) -> some View {
        AgentChatTranscriptView(
            session: session,
            scrolledMessageID: $scrolledMessageID,
            onBatchTap: { presentedBatch = $0 },
            onRetry: { session.retry() },
            onRegenerate: { session.regenerateLast() }
        )
    }

    @ViewBuilder
    private var emptyState: some View {
        if showSettingsHint {
            AgentChatDisconnectedView()
        } else {
            AgentChatWelcomeView(draft: $draft, inputFocused: $inputFocused)
        }
    }

    private func composer(session: AgentChatSession) -> some View {
        VStack(spacing: AppTheme.Spacing.xs) {
            errorBanner(session: session)
            inputRow(session: session)
        }
        .background(.ultraThinMaterial)
    }

    @ViewBuilder
    private func errorBanner(session: AgentChatSession) -> some View {
        if case .failed(let msg) = session.phase {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(AppTheme.Tint.warning)
                    .font(AppTheme.Typography.caption)
                    .accessibilityHidden(true)
                Text(msg)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                Spacer(minLength: 0)
                if session.lastFailedMessage != nil {
                    Button("Retry") {
                        Haptics.selection()
                        session.retry()
                    }
                    .buttonStyle(.glass)
                    .controlSize(.small)
                }
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.xs)
        }
    }

    private func inputRow(session: AgentChatSession) -> some View {
        VStack(alignment: .trailing, spacing: 2) {
            HStack(alignment: .bottom, spacing: AppTheme.Spacing.sm) {
                TextField("Message your agent…", text: $draft, axis: .vertical)
                    .textFieldStyle(.plain)
                    .focused($inputFocused)
                    .lineLimit(1...5)
                    .padding(.horizontal, Layout.inputFieldPaddingH)
                    .padding(.vertical, Layout.inputFieldPaddingV)
                    .glassEffect(.regular, in: .rect(cornerRadius: Layout.inputFieldCornerRadius))
                    .disabled(showSettingsHint)

                if case .sending = session.phase {
                    stopButton(session: session)
                } else {
                    sendButton(session: session)
                }
            }
            if draft.count > Layout.charCountThreshold {
                Text("\(draft.count)")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.tertiary)
                    .monospacedDigit()
                    .contentTransition(.numericText())
                    .animation(AppTheme.Animation.springFast, value: draft.count)
                    .padding(.trailing, Layout.sendButtonSize + AppTheme.Spacing.sm)
            }
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.vertical, AppTheme.Spacing.sm)
        .animation(AppTheme.Animation.springFast, value: draft.count > Layout.charCountThreshold)
        .animation(AppTheme.Animation.springFast, value: session.phase == .sending)
    }

    private func sendButton(session: AgentChatSession) -> some View {
        Button {
            sendCurrentDraft()
        } label: {
            Image(systemName: "arrow.up")
                .font(.system(size: Layout.sendButtonIconSize, weight: .bold))
                .foregroundStyle(.white)
                .frame(width: Layout.sendButtonSize, height: Layout.sendButtonSize)
                .background(AppTheme.Gradients.agentAccent, in: .circle)
                .opacity(canSend(session: session) ? 1.0 : 0.4)
        }
        .buttonStyle(.pressable)
        .keyboardShortcut(.return, modifiers: .command)
        .accessibilityLabel("Send message")
        .disabled(!canSend(session: session))
    }

    private func stopButton(session: AgentChatSession) -> some View {
        Button {
            Haptics.selection()
            session.cancelSend()
        } label: {
            Image(systemName: "stop.fill")
                .font(.system(size: Layout.sendButtonIconSize - 2, weight: .bold))
                .foregroundStyle(.white)
                .frame(width: Layout.sendButtonSize, height: Layout.sendButtonSize)
                .background(AppTheme.Gradients.agentAccent, in: .circle)
        }
        .buttonStyle(.pressable)
        .accessibilityLabel("Stop generating")
    }

    private func canSend(session: AgentChatSession) -> Bool {
        guard !showSettingsHint else { return false }
        guard session.canSend else { return false }
        return !draft.isBlank
    }

    private func sendCurrentDraft() {
        guard canSend(session: session) else { return }
        let text = draft
        draft = ""
        didSendInSession = true
        Haptics.light()
        session.startSend(text)
    }

    private var background: LinearGradient {
        AppTheme.Gradients.agentChatBackground
    }
}
