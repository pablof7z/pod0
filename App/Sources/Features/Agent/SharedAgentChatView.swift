import Foundation
import Pod0Core
import SwiftUI

/// Native SwiftUI shell over the Rust-owned durable conversation workflow.
struct SharedAgentChatView: View {
    let session: SharedAgentConversationSession
    let requestedConversationID: ConversationId?
    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback
    @State private var draft = ""
    @State private var showHistory = false
    @FocusState private var inputFocused: Bool

    var body: some View {
        ZStack {
            AppTheme.Gradients.agentChatBackground.ignoresSafeArea()
            VStack(spacing: 0) {
                if visibleMessages.isEmpty {
                    welcome
                } else {
                    SharedAgentChatTranscript(
                        messages: visibleMessages,
                        streamingContent: session.streamingContent,
                        isRunning: session.phase == .running,
                        onOpenRecallEvidence: openRecallEvidence
                    )
                }
                composer
            }
        }
        .navigationTitle("Agent")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { toolbar }
        .sheet(isPresented: $showHistory) {
            AgentChatHistoryView(
                conversations: session.conversationSummaries,
                hasMore: session.conversationHistoryHasMore,
                currentID: session.conversationID,
                onSelect: openConversation,
                onNew: startNewConversation
            )
        }
        .onAppear {
            selectRequestedConversation()
            drainPendingContext()
            inputFocused = hasCredential
        }
        .onChange(of: requestedConversationID) { _, _ in
            selectRequestedConversation()
        }
        .onChange(of: session.phase) { _, phase in
            switch phase {
            case .idle: Haptics.success()
            case .failed: Haptics.error()
            case .running: break
            }
        }
    }

    private var visibleMessages: [ChatMessage] {
        return SharedAgentChatMessageMapper.messages(from: session.turns) { episodeID in
            guard let episode = store.episode(id: episodeID) else { return nil }
            return RecallEvidenceMetadata(
                episodeTitle: episode.title,
                podcastTitle: store.podcast(id: episode.podcastID)?.title ?? "Unknown podcast"
            )
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button {
                session.refreshConversationHistory()
                showHistory = true
            } label: {
                Image(systemName: "clock.arrow.circlepath")
            }
            .accessibilityLabel("Conversation history")
        }
        ToolbarItem(placement: .primaryAction) {
            Button(action: startNewConversation) {
                Image(systemName: "square.and.pencil")
            }
            .accessibilityLabel("New conversation")
        }
        if !visibleMessages.isEmpty {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    if let url = AgentChatTranscriptExport.write(
                        visibleMessages,
                        batchSummaries: [:]
                    ) {
                        SystemShareSheet.present(items: [url])
                    }
                } label: {
                    Image(systemName: "square.and.arrow.up")
                }
                .accessibilityLabel("Export transcript")
            }
        }
    }

    private var welcome: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Spacer()
            Image(systemName: "sparkles")
                .font(.system(size: 44, weight: .semibold))
                .foregroundStyle(AppTheme.Gradients.agentAccent)
            Text("What do you want to know?")
                .font(AppTheme.Typography.title)
            Text("Ask about your library, save a note, or control playback.")
                .font(AppTheme.Typography.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            ForEach(Self.suggestions, id: \.self) { suggestion in
                Button(suggestion) {
                    draft = suggestion
                    inputFocused = true
                }
                .buttonStyle(.glass)
            }
            Spacer()
        }
        .padding(AppTheme.Spacing.lg)
    }

    private var composer: some View {
        VStack(spacing: AppTheme.Spacing.xs) {
            if case .failed(let detail) = session.phase {
                Text(detail)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(AppTheme.Tint.warning)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack(alignment: .bottom, spacing: AppTheme.Spacing.sm) {
                TextField("Message your agent…", text: $draft, axis: .vertical)
                    .textFieldStyle(.plain)
                    .focused($inputFocused)
                    .lineLimit(1...5)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .glassEffect(.regular, in: .rect(cornerRadius: 22))
                    .disabled(!hasCredential)
                Button {
                    if session.phase == .running {
                        Task { await session.cancelActiveTurn() }
                    } else {
                        send()
                    }
                } label: {
                    Image(systemName: session.phase == .running ? "stop.fill" : "arrow.up")
                        .font(.system(size: 16, weight: .bold))
                        .foregroundStyle(.white)
                        .frame(width: 38, height: 38)
                        .background(AppTheme.Gradients.agentAccent, in: .circle)
                }
                .buttonStyle(.pressable)
                .disabled(session.phase != .running && !canSend)
                .accessibilityLabel(session.phase == .running ? "Stop generating" : "Send message")
            }
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.vertical, AppTheme.Spacing.sm)
        .background(.ultraThinMaterial)
    }

    private var canSend: Bool {
        hasCredential && session.canSend && !draft.isBlank
    }

    private var hasCredential: Bool {
        let reference = LLMModelReference(storedID: store.state.settings.agentInitialModel)
        return LLMProviderCredentialResolver.hasAPIKey(for: reference.provider)
    }

    private func send() {
        guard canSend else { return }
        let input = draft
        draft = ""
        Task { await session.startTurn(input) }
    }

    private func openRecallEvidence(_ evidence: RecallEvidence) {
        _ = RecallPlaybackHandoff.open(evidence, store: store, playback: playback)
    }

    private func startNewConversation() {
        session.startNewConversation()
        draft = ""
        inputFocused = true
    }

    private func openConversation(_ conversationID: ConversationId) {
        session.openConversation(conversationID)
    }

    private func selectRequestedConversation() {
        guard let requestedConversationID else { return }
        session.openConversation(requestedConversationID)
    }

    private func drainPendingContext() {
        if let voice = store.pendingVoiceNoteAgentContext {
            store.pendingVoiceNoteAgentContext = nil
            store.pendingChapterAgentContext = nil
            Task { await session.startTurn(voice.prefilledDraft) }
        } else if let chapter = store.pendingChapterAgentContext {
            draft = chapter.prefilledDraft
            store.pendingChapterAgentContext = nil
        }
    }

    private static let suggestions = [
        "What's new in my library?",
        "What should I listen to next?",
        "Save a note about what I'm hearing",
    ]
}
