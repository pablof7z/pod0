import SwiftUI

// MARK: - GenerateWikiSheet

/// Modal sheet over `WikiView` — collects a topic from the user and
/// dispatches `podcast.wiki.generate { podcast_id, topic }`. The kernel
/// returns a fresh `article_id` in the envelope; we close the sheet on
/// success regardless and let the snapshot tick render the new row.
struct GenerateWikiSheet: View {
    @Environment(KernelModel.self) private var model
    @FocusState private var topicFocused: Bool

    let podcastId: String
    let onDismiss: () -> Void

    @State private var topic: String = ""

    var body: some View {
        NavigationStack {
            VStack(spacing: AppTheme.Spacing.lg) {
                Text("What topic should this article cover? The wiki entry will be synthesised from this show's transcripts and supporting web research.")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)

                TextField("Topic (e.g. Bitcoin halvings)", text: $topic, axis: .vertical)
                    .focused($topicFocused)
                    .textFieldStyle(.roundedBorder)
                    .submitLabel(.go)
                    .onSubmit(generate)

                Spacer(minLength: 0)
            }
            .padding(.horizontal, AppTheme.Spacing.lg)
            .padding(.top, AppTheme.Spacing.lg)
            .navigationTitle("Generate Article")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel", action: onDismiss)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Generate", action: generate)
                        .disabled(!canSubmit)
                        .fontWeight(.semibold)
                }
            }
            .onAppear { topicFocused = true }
        }
    }

    private var canSubmit: Bool {
        !topic.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func generate() {
        let trimmed = topic.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        Haptics.success()
        model.dispatch(
            namespace: "podcast.wiki",
            body: ["op": "generate", "podcast_id": podcastId, "topic": trimmed]
        )
        onDismiss()
    }
}
