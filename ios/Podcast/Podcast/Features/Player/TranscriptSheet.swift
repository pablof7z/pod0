import SwiftUI

// MARK: - TranscriptSheet
//
// Sheet that renders the plain-text transcript for one episode. The transcript
// itself is owned by the Rust kernel: opening the sheet dispatches
// `podcast.fetch_transcript`; the result lands on the next snapshot tick as
// `EpisodeSummary.transcript`. While the fetch is in flight (and after a
// silent "not available" outcome) the view shows an unavailable state.
//
// Doctrine:
//   D2 — no business logic in the shell. The kernel decides whether a
//        transcript exists, how to fetch it, how to parse it, and what plain
//        text to surface. The shell only renders.
//   D5 — no default-zero render. When the kernel says `nil` the empty state
//        is shown explicitly; we do not invent placeholder paragraphs.

struct TranscriptSheet: View {
    let episode: EpisodeSummary
    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    /// Re-read the episode from `model.library` on every tick so the
    /// transcript text updates as soon as the kernel writes it. Falls back
    /// to the caller-supplied row when the episode disappears from the
    /// library (e.g. the user unsubscribed while the sheet is open).
    private var liveEpisode: EpisodeSummary {
        model.library
            .flatMap { $0.episodes }
            .first { $0.id == episode.id }
            ?? episode
    }

    var body: some View {
        NavigationStack {
            Group {
                if let text = liveEpisode.transcript, !text.isEmpty {
                    transcriptView(text: text)
                } else {
                    unavailableView
                }
            }
            .navigationTitle("Transcript")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .task {
            // Kick off the fetch when the sheet opens. Fire-and-forget —
            // the result arrives on the next snapshot tick, so the view
            // re-renders automatically.
            model.dispatch(
                namespace: "podcast",
                body: ["op": "fetch_transcript", "episode_id": episode.id]
            )
        }
    }

    // MARK: - Subviews

    private func transcriptView(text: String) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
                Text(liveEpisode.title)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.secondary)

                Text(text)
                    .font(AppTheme.Typography.body)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(AppTheme.Spacing.lg)
        }
    }

    private var unavailableView: some View {
        ContentUnavailableView(
            "No Transcript",
            systemImage: "text.bubble",
            description: Text("Transcript not available for this episode.")
        )
    }
}
