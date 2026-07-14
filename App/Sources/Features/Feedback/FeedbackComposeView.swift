import SwiftUI

// MARK: - FeedbackComposeView

struct FeedbackComposeView: View {

    private enum Layout {
        static let identityIconSize: CGFloat = 20
        static let textEditorMinHeight: CGFloat = 200
        static let characterLimit: Int = 280
    }
    let store: FeedbackStore
    @Bindable var workflow: FeedbackWorkflow
    @Environment(\.dismiss) private var dismiss
    @Environment(UserIdentityStore.self) private var userIdentity

    @State private var errorMessage: String?
    @State private var showDiscardConfirm = false

    private let characterLimit = Layout.characterLimit

    private var characterCount: Int { workflow.draft.count }
    private var charactersRemaining: Int { characterLimit - characterCount }
    private var isOverLimit: Bool { characterCount > characterLimit }

    private var canSend: Bool {
        !workflow.draft.isBlank && !isOverLimit
    }

    var body: some View {
        NavigationStack {
            ZStack(alignment: .topLeading) {
                Color(.systemBackground)
                    .ignoresSafeArea()

                VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                    identityRow
                    textEditorSection
                    characterCounterRow
                    screenshotSection

                    if let error = errorMessage {
                        Text(error)
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(AppTheme.Tint.error)
                            .transition(.opacity)
                    }

                    Spacer()
                }
                .padding(AppTheme.Spacing.md)
            }
            .navigationTitle("New Feedback")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { composeToolbar }
            // iOS-standard discard confirmation. Cancel without a
            // confirm step on a non-empty draft was the most painful
            // foot-gun in this view: a misfired tap wiped a long
            // feedback message + any screenshot the user had taken
            // and annotated, with no undo.
            .confirmationDialog(
                "Discard this feedback?",
                isPresented: $showDiscardConfirm,
                titleVisibility: .visible
            ) {
                Button("Discard", role: .destructive) { discardAndDismiss() }
                Button("Keep Editing", role: .cancel) {}
            } message: {
                Text("Your draft and any attached screenshot will be lost.")
            }
        }
    }

    // MARK: - Identity row

    @ViewBuilder
    private var identityRow: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: userIdentity.hasIdentity ? "person.crop.circle.fill" : "person.crop.circle")
                .font(.system(size: Layout.identityIconSize))
                .foregroundStyle(userIdentity.hasIdentity ? Color(.label) : Color(.tertiaryLabel))

            if let short = userIdentity.npubShort {
                Text(short)
                    .font(AppTheme.Typography.mono)
                    .foregroundStyle(.secondary)
            } else {
                Text("Anonymous — tap to set identity")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Text("Posting as")
                .font(AppTheme.Typography.caption2)
                .foregroundStyle(.tertiary)
        }
        .padding(AppTheme.Spacing.sm)
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var composeToolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarLeading) {
            Button("Cancel") { cancel() }
        }
        ToolbarItem(placement: .topBarLeading) {
            screenshotToolbarButton
        }
        ToolbarItem(placement: .confirmationAction) {
            AsyncButton(
                action: { try await send() },
                onError: { error in
                    errorMessage = error.localizedDescription
                    Haptics.error()
                }
            ) {
                Text("Send")
            }
            .fontWeight(.semibold)
            .disabled(!canSend)
        }
    }

    @ViewBuilder
    private var screenshotToolbarButton: some View {
        let hasImage = workflow.annotatedImage != nil || workflow.screenshot != nil
        Button {
            if hasImage {
                workflow.phase = .annotating
                dismiss()
            } else {
                workflow.phase = .awaitingScreenshot
                dismiss()
            }
        } label: {
            Image(systemName: hasImage ? "camera.viewfinder" : "camera")
                .symbolVariant(hasImage ? .fill : .none)
                .foregroundStyle(hasImage ? .blue : .secondary)
        }
        .accessibilityLabel(hasImage ? "Re-annotate screenshot" : "Attach screenshot")
    }

    // MARK: - Text editor

    @ViewBuilder
    private var textEditorSection: some View {
        ZStack(alignment: .topLeading) {
            if workflow.draft.isEmpty {
                Text("What's on your mind?")
                    .foregroundStyle(.tertiary)
                    .padding(AppTheme.Spacing.md)
            }

            TextEditor(text: $workflow.draft)
                .frame(minHeight: Layout.textEditorMinHeight)
                .scrollContentBackground(.hidden)
                .padding(AppTheme.Spacing.sm)
        }
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))
    }

    // MARK: - Character counter

    private var characterCounterRow: some View {
        HStack {
            Spacer()
            HStack(spacing: AppTheme.Spacing.xs) {
                if isOverLimit || charactersRemaining <= 40 {
                    Text("\(charactersRemaining)")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(isOverLimit ? AppTheme.Tint.error : charactersRemaining <= 15 ? AppTheme.Tint.warning : .secondary)
                        .monospacedDigit()
                        .contentTransition(.numericText(countsDown: true))
                        .animation(AppTheme.Animation.springFast, value: charactersRemaining)
                }
                ZStack {
                    Circle()
                        .stroke(Color.secondary.opacity(0.2), lineWidth: 2.5)
                    Circle()
                        .trim(from: 0, to: min(Double(characterCount) / Double(characterLimit), 1.0))
                        .stroke(counterProgressColor, style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
                        .rotationEffect(.degrees(-90))
                }
                .frame(width: 20, height: 20)
                .animation(AppTheme.Animation.springFast, value: characterCount)
            }
        }
    }

    private var counterProgressColor: Color {
        if isOverLimit { return AppTheme.Tint.error }
        let ratio = Double(characterCount) / Double(characterLimit)
        if ratio >= 0.90 { return AppTheme.Tint.error }
        if ratio >= 0.75 { return AppTheme.Tint.warning }
        return Color.accentColor
    }

    // MARK: - Screenshot preview

    @ViewBuilder
    private var screenshotSection: some View {
        let displayImage = workflow.annotatedImage ?? workflow.screenshot
        if let image = displayImage {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md))
                    .overlay(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.md)
                            .strokeBorder(Color(.separator), lineWidth: 0.5)
                    )

                screenshotActionRow
            }
        }
    }

    private var screenshotActionRow: some View {
        HStack {
            Button("Re-annotate") {
                workflow.phase = .annotating
                dismiss()
            }
            .foregroundStyle(.blue)

            Spacer()

            Button("Remove") {
                workflow.screenshot = nil
                workflow.annotatedImage = nil
            }
            .foregroundStyle(AppTheme.Tint.error)
        }
        .font(AppTheme.Typography.caption)
    }

    // MARK: - Actions

    /// True when there's user-entered content the system would lose if
    /// we discarded right now. Drives whether Cancel is a one-tap
    /// dismiss or routes through the confirmation dialog.
    private var hasUnsavedWork: Bool {
        !workflow.draft.isBlank
            || workflow.screenshot != nil
            || workflow.annotatedImage != nil
    }

    private func cancel() {
        if hasUnsavedWork {
            showDiscardConfirm = true
            return
        }
        discardAndDismiss()
    }

    private func discardAndDismiss() {
        workflow.phase = .idle
        workflow.draft = ""
        workflow.screenshot = nil
        workflow.annotatedImage = nil
        dismiss()
    }

    private func send() async throws {
        Haptics.light()
        errorMessage = nil

        let image = workflow.annotatedImage ?? workflow.screenshot
        try await store.publishThread(
            category: workflow.selectedCategory,
            content: workflow.draft.trimmed,
            image: image,
            identity: userIdentity
        )
        Haptics.success()
        workflow.phase = .idle
        workflow.draft = ""
        workflow.screenshot = nil
        workflow.annotatedImage = nil
        dismiss()
    }
}
